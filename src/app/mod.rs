pub(crate) mod cli;

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::env;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::app::cli::CliCommand;
use crate::display::{
    build_info, display_state_from_events, event_label, status_output, BuildInfo, StatusOutput,
};
use crate::event::store::{read_state, state_path, with_state_update};
use crate::event::{
    append_and_trim, capture_decision, clear_read_events, empty_state,
    mark_focused_window_events_read, prune_stale_events, set_event_status, Agent, AgentEvent,
    AgentNotifierState, CaptureDecision, EventStatus, FocusOutcome, SourceLiveness, SourceWindow,
};
use crate::exec::{run_command, run_command_owned, DEFAULT_TIMEOUT};
use crate::intake;
use crate::window::{hyprland, proc};
use crate::{STATUS_ERROR_CLASS, UNAVAILABLE_STATUS_JSON, UNAVAILABLE_STATUS_TOOLTIP};

fn mark_address_read(address: &str, now: DateTime<Utc>) -> io::Result<()> {
    let _ = with_state_update(&state_path()?, now, |state| {
        mark_focused_window_events_read(state, Some(address))
    })?;
    Ok(())
}

fn compositor_liveness() -> SourceLiveness {
    SourceLiveness {
        existing_addresses: hyprland::existing_window_addresses(),
        process_is_alive: proc::process_is_alive,
    }
}

fn focusable_events(events: &[AgentEvent]) -> Vec<AgentEvent> {
    crate::event::focusable_events(events, &compositor_liveness())
}

fn read_state_with_focused_window_read(
    path: &Path,
    focused: Option<&str>,
    now: DateTime<Utc>,
) -> io::Result<AgentNotifierState> {
    with_state_update(path, now, |state| {
        mark_focused_window_events_read(state, focused)
    })
}

fn format_status(state: &AgentNotifierState) -> StatusOutput {
    status_output(&focusable_events(&state.events))
}

fn prefix_share_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .and_then(|dir| dir.parent().map(Path::to_path_buf))
        .map_or_else(installed_share_dir, |dir| dir.join("share/agent-notifier"))
}

fn installed_share_dir() -> PathBuf {
    env::var_os("HOME")
        .map_or_else(|| PathBuf::from(""), PathBuf::from)
        .join(".local/share/agent-notifier")
}

fn share_dir() -> PathBuf {
    if let Some(override_dir) = env::var_os("AGENT_NOTIFIER_SHARE_DIR") {
        if !override_dir.is_empty() {
            return PathBuf::from(override_dir);
        }
    }
    let exe = env::current_exe().unwrap_or_default();
    if exe.to_string_lossy().contains("/.local/") {
        installed_share_dir()
    } else {
        prefix_share_dir()
    }
}

fn sound_file() -> PathBuf {
    if let Some(file) = env::var_os("AGENT_NOTIFIER_SOUND_FILE") {
        if !file.is_empty() {
            return PathBuf::from(file);
        }
    }
    env::var_os("AGENT_NOTIFIER_SOUND_DIR")
        .map_or_else(share_dir, PathBuf::from)
        .join("agent-complete.mp3")
}

fn notify(event: &AgentEvent) {
    let agent_name = Agent::from_id(&event.agent).display_name();
    let _ = run_command_owned(
        &[
            "notify-send".to_owned(),
            format!("--app-name={agent_name}"),
            format!("{agent_name} completed"),
            event_label(event),
        ],
        DEFAULT_TIMEOUT,
    );
}

fn play_sound() {
    if env::var("AGENT_NOTIFIER_SOUND").as_deref() == Ok("0") {
        return;
    }
    let file = sound_file().to_string_lossy().into_owned();
    if run_command(
        &["mpv", "--no-video", "--really-quiet", &file],
        DEFAULT_TIMEOUT,
    )
    .unwrap_or(1)
        == 0
    {
        return;
    }
    let _ = run_command(&["canberra-gtk-play", "-f", &file], DEFAULT_TIMEOUT);
}

fn capture_completion_event(event: &AgentEvent, now: DateTime<Utc>) -> io::Result<()> {
    match capture_decision(event, hyprland::focused_window_address().as_deref()) {
        CaptureDecision::Discard => return Ok(()),
        CaptureDecision::PersistAndAlert => {
            with_state_update(&state_path()?, now, |state| {
                append_and_trim(state, event.clone())
            })?;
        }
        CaptureDecision::AlertOnly => {
            eprintln!(
                "agent-notifier: no Hyprland client address for this completion; alerting without storing"
            );
        }
    }
    let notify_event = event.clone();
    let sound = thread::spawn(play_sound);
    let notification = thread::spawn(move || notify(&notify_event));
    let _ = sound.join();
    let _ = notification.join();
    Ok(())
}

/// Resolve the source window, retrying once when the address is missing —
/// `hyprctl clients` can race a window that has just been mapped.
fn resolve_source_window() -> Option<SourceWindow> {
    let first = hyprland::current_source_window();
    if first
        .as_ref()
        .is_some_and(|source_window| source_window.client_address.is_some())
    {
        return first;
    }
    thread::sleep(Duration::from_millis(100));
    hyprland::current_source_window().or(first)
}

fn handle_agent_hook(agent: Agent) -> io::Result<()> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let now = Utc::now();
    let event = intake::capture(agent, &raw, resolve_source_window(), now);
    capture_completion_event(&event, now)
}

fn handle_status_json() -> io::Result<()> {
    let focused = hyprland::focused_window_address();
    let state =
        read_state_with_focused_window_read(&state_path()?, focused.as_deref(), Utc::now())?;
    let output = std::panic::catch_unwind(|| format_status(&state))
        .unwrap_or_else(|_| unavailable_status_output());
    print_json(&output);
    Ok(())
}

fn handle_focused_window_read() -> io::Result<()> {
    let Some(address) = hyprland::focused_window_address() else {
        return Ok(());
    };
    mark_address_read(&address, Utc::now())
}

fn handle_watch_focused_window() -> io::Result<()> {
    hyprland::watch_focused_window(|address| {
        if let Err(error) = mark_address_read(address, Utc::now()) {
            eprintln!("agent-notifier: state update failed: {error}");
        }
    })
}

fn focus_event(event: Option<&AgentEvent>, target: &str) -> io::Result<i32> {
    match hyprland::focus_event_source(event) {
        FocusOutcome::NotFocused => {
            eprintln!("agent-notifier: could not focus the source window for {target}");
            Ok(1)
        }
        FocusOutcome::Primary => {
            if let Some(id) = event.map(|event| event.id.clone()) {
                let _ = with_state_update(&state_path()?, Utc::now(), |state| {
                    set_event_status(state, &id, EventStatus::Read)
                })?;
            }
            Ok(0)
        }
        FocusOutcome::Fallback => Ok(0),
    }
}

fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string(value) {
        Ok(json) => println!("{json}"),
        Err(_) => {
            println!("{UNAVAILABLE_STATUS_JSON}");
        }
    }
}

fn crate_build_info() -> BuildInfo {
    let state_path = state_path().ok();
    build_info(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("AGENT_NOTIFIER_COMMIT"),
        env!("AGENT_NOTIFIER_DIRTY"),
        env!("AGENT_NOTIFIER_COMMIT_DATE"),
        state_path.as_deref(),
    )
}

fn unavailable_status_output() -> StatusOutput {
    StatusOutput {
        text: "agents !".to_owned(),
        tooltip: UNAVAILABLE_STATUS_TOOLTIP.to_owned(),
        class: STATUS_ERROR_CLASS.to_owned(),
    }
}

fn usage() -> &'static str {
    "Usage: agent-notifier <command>

Commands:
  hook                     Capture a Codex completion from stdin
  pi-hook                  Capture a Pi completion from stdin
  claude-hook              Capture a Claude Code completion from stdin
  status-json              Print bar-widget status JSON
  list-display-json        Print focusable events as display JSON
  version-json             Print build metadata as JSON
  focus-id <event-id>      Focus an event by id
  mark-read <event-id>     Mark an event as read
  watch-focused-window     Watch focused-window changes
  clear-read               Remove read events
  clear-all                Remove all events
  prune-stale              Remove events whose source window is gone

Options:
  -h, --help               Print help
  -V, --version            Print version"
}

pub(crate) fn run() -> io::Result<i32> {
    match CliCommand::from_env() {
        CliCommand::Help => {
            println!("{}", usage());
            Ok(0)
        }
        CliCommand::Version => {
            println!("agent-notifier {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        CliCommand::Hook => handle_agent_hook(Agent::Codex).map(|()| 0),
        CliCommand::PiHook => handle_agent_hook(Agent::Pi).map(|()| 0),
        CliCommand::ClaudeHook => handle_agent_hook(Agent::Claude).map(|()| 0),
        CliCommand::StatusJson => handle_status_json().map(|()| 0),
        // TODO(contract): no known consumer — retire or test before v1.
        CliCommand::ListJson => {
            print_json(&read_state(&state_path()?, Utc::now())?);
            Ok(0)
        }
        CliCommand::ListDisplayJson => {
            let focused = hyprland::focused_window_address();
            let state = read_state_with_focused_window_read(
                &state_path()?,
                focused.as_deref(),
                Utc::now(),
            )?;
            print_json(&display_state_from_events(
                state.version,
                focusable_events(&state.events),
            ));
            Ok(0)
        }
        CliCommand::VersionJson => {
            print_json(&crate_build_info());
            Ok(0)
        }
        // TODO(contract): no known consumer — retire or test before v1.
        CliCommand::FocusLatest => {
            let state = read_state(&state_path()?, Utc::now())?;
            let focusable = focusable_events(&state.events);
            let event = focusable
                .iter()
                .find(|event| event.status == EventStatus::Unread);
            focus_event(event, "the latest unread event")
        }
        CliCommand::FocusId(id) => {
            let state = read_state(&state_path()?, Utc::now())?;
            let event = state.events.iter().find(|event| event.id == id);
            focus_event(event, &id)
        }
        CliCommand::MarkRead(id) => {
            let _ = with_state_update(&state_path()?, Utc::now(), |state| {
                set_event_status(state, &id, EventStatus::Read)
            })?;
            Ok(0)
        }
        CliCommand::FocusIdMissing => {
            eprintln!("agent-notifier: focus-id requires an event id");
            Ok(2)
        }
        CliCommand::MarkReadMissing => {
            eprintln!("agent-notifier: mark-read requires an event id");
            Ok(2)
        }
        // TODO(contract): no known consumer — retire or test before v1.
        CliCommand::FocusedWindowRead => handle_focused_window_read().map(|()| 0),
        CliCommand::WatchFocusedWindow => handle_watch_focused_window().map(|()| 0),
        CliCommand::ClearRead => {
            let _ = with_state_update(&state_path()?, Utc::now(), clear_read_events)?;
            Ok(0)
        }
        CliCommand::ClearAll => {
            let _ = with_state_update(&state_path()?, Utc::now(), |_| empty_state())?;
            Ok(0)
        }
        CliCommand::PruneStale => {
            let liveness = SourceLiveness {
                existing_addresses: hyprland::try_existing_window_addresses()?,
                process_is_alive: proc::process_is_alive,
            };
            let _ = with_state_update(&state_path()?, Utc::now(), |state| {
                prune_stale_events(state, &liveness)
            })?;
            Ok(0)
        }
        CliCommand::Unknown => {
            eprintln!("{}", usage());
            Ok(2)
        }
    }
}

#[cfg(test)]
mod tests;
