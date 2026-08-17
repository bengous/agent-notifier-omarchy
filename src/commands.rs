use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashSet;
use std::env;
use std::io::{self, BufRead, BufReader, Read};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::cli::CliCommand;
use crate::pi_event::{build_pi_event, parse_pi_hook_input};
use crate::presentation::{
    build_info, display_state_from_events, event_label, status_output, BuildInfo, StatusOutput,
};
use crate::process::{run_command, run_command_owned, DEFAULT_TIMEOUT};
use crate::state::{
    append_and_trim, clear_read_events, dedupe_events, empty_state, set_event_status,
    set_window_address_read, state_has_unread_for_address, AgentEvent, AgentNotifierState,
    EventStatus, FocusOutcome, SourceWindow,
};
use crate::stop_event::{
    build_stop_event, current_git_branch, parse_stop_hook_input, project_root, random_hex,
    repository_key, StopHookInput,
};
use crate::storage::{read_state_or_recover, state_path, with_state_update};
use crate::{hyprland, session_title};
use crate::{STATUS_ERROR_CLASS, UNAVAILABLE_STATUS_JSON, UNAVAILABLE_STATUS_TOOLTIP};

fn set_focused_window_read(state: AgentNotifierState) -> AgentNotifierState {
    match hyprland::focused_window_address() {
        Some(address) => set_window_address_read(state, &address),
        None => state,
    }
}

fn mark_address_read(address: &str, now: DateTime<Utc>) -> io::Result<bool> {
    let mut changed = false;
    let _ = with_state_update(&state_path()?, now, |state| {
        changed = state_has_unread_for_address(&state, address);
        if changed {
            set_window_address_read(state, address)
        } else {
            state
        }
    })?;
    Ok(changed)
}

fn focusable_events(events: &[AgentEvent]) -> Vec<AgentEvent> {
    focusable_events_for_addresses(events, &hyprland::existing_window_addresses())
}

fn focusable_events_for_addresses(
    events: &[AgentEvent],
    existing_addresses: &HashSet<String>,
) -> Vec<AgentEvent> {
    let focusable = events
        .iter()
        .filter(|event| event_has_live_source(event, existing_addresses))
        .cloned()
        .collect::<Vec<_>>();
    dedupe_events(focusable)
}

/// The source process is the per-window death signal: closing the window kills
/// its shell even when the terminal shares one pid across windows. Events
/// without one (legacy state) fall back to window-address liveness.
fn event_has_live_source(event: &AgentEvent, existing_addresses: &HashSet<String>) -> bool {
    event
        .workspace
        .as_ref()
        .is_some_and(|workspace| match &workspace.source_process {
            Some(process) => hyprland::process_is_alive(process),
            None => workspace
                .candidate_addresses()
                .iter()
                .any(|address| existing_addresses.contains(*address)),
        })
}

fn prune_stale_events(
    mut state: AgentNotifierState,
    existing_addresses: &HashSet<String>,
) -> AgentNotifierState {
    state
        .events
        .retain(|event| event_has_live_source(event, existing_addresses));
    state
}

fn read_state_with_focused_window_read(now: DateTime<Utc>) -> io::Result<AgentNotifierState> {
    let path = state_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let current = read_state_or_recover(&path, now)?;
    let next = set_focused_window_read(current.clone());
    if next == current {
        return Ok(current);
    }
    with_state_update(&path, now, set_focused_window_read)
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

/// Only events whose source window we can address again are worth persisting.
fn should_capture_event(event: &AgentEvent) -> bool {
    event
        .workspace
        .as_ref()
        .is_some_and(|workspace| workspace.client_address.is_some())
}

fn agent_display_name(agent: &str) -> &str {
    match agent {
        "pi" => "Pi",
        "claude" => "Claude",
        _ => "Codex",
    }
}

fn notify(event: &AgentEvent) {
    let agent_name = agent_display_name(&event.agent);
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

fn fallback_cwd() -> String {
    env::var("PWD")
        .ok()
        .or_else(|| {
            env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| ".".to_owned())
}

fn capture_completion_event(event: &AgentEvent, now: DateTime<Utc>) -> io::Result<()> {
    if hyprland::is_focused_source_event(event) {
        return Ok(());
    }
    if should_capture_event(event) {
        with_state_update(&state_path()?, now, |state| {
            append_and_trim(state, event.clone())
        })?;
    } else {
        // Without an address we could never focus or expire this event, so it is
        // not persisted. The alert still fires: losing the notification entirely
        // is the one failure this tool exists to prevent. Note this path bypasses
        // deduplication, because nothing is stored to deduplicate against.
        eprintln!(
            "agent-notifier: no Hyprland client address for this completion; alerting without storing"
        );
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

fn hook_session_id(event: &AgentEvent) -> Option<&str> {
    Some(event.session_id.as_str()).filter(|id| !id.is_empty() && *id != "unknown")
}

fn resolve_session_title(
    agent: &str,
    input: &StopHookInput,
    session_id: Option<&str>,
) -> Option<String> {
    if agent == "claude" {
        let transcript_path = input.transcript_path.as_deref()?;
        return session_title::claude_session_title(Path::new(transcript_path), session_id);
    }
    session_title::codex_session_title(&session_title::codex_sessions_dir()?, session_id?)
}

fn handle_hook(agent: &str) -> io::Result<()> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let input = parse_stop_hook_input(&raw);
    let cwd = input.cwd.clone().unwrap_or_else(fallback_cwd);
    let now = Utc::now();
    let project_path = project_root(&cwd);
    let project_key = repository_key(&cwd, &project_path);
    let branch_name = current_git_branch(&project_path);
    let event = build_stop_event(
        agent,
        &input,
        cwd.clone(),
        project_path,
        project_key,
        branch_name,
        resolve_source_window(),
        now,
        &random_hex(4),
    );
    let session_title = resolve_session_title(agent, &input, hook_session_id(&event));
    let event = AgentEvent {
        session_title,
        ..event
    };
    capture_completion_event(&event, now)
}

fn handle_pi_hook() -> io::Result<()> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let input = parse_pi_hook_input(&raw);
    let cwd = input.cwd.clone().unwrap_or_else(fallback_cwd);
    let now = Utc::now();
    let project_path = project_root(&cwd);
    let project_key = repository_key(&cwd, &project_path);
    let branch_name = current_git_branch(&project_path);
    let event = build_pi_event(
        &input,
        cwd.clone(),
        project_path,
        project_key,
        branch_name,
        resolve_source_window(),
        now,
        &random_hex(4),
    );
    capture_completion_event(&event, now)
}

fn handle_status_json() -> io::Result<()> {
    let state = read_state_with_focused_window_read(Utc::now())?;
    let output = std::panic::catch_unwind(|| format_status(&state))
        .unwrap_or_else(|_| unavailable_status_output());
    print_json(&output);
    Ok(())
}

fn handle_focused_window_read() -> io::Result<()> {
    let Some(address) = hyprland::focused_window_address() else {
        return Ok(());
    };
    mark_address_read(&address, Utc::now())?;
    Ok(())
}

fn parse_focused_window_address(line: &str) -> Option<String> {
    let payload = line.strip_prefix("activewindowv2>>")?.trim();
    if payload.is_empty() || payload == "," {
        return None;
    }
    // hyprctl reports `0x…`; the socket payload may omit the prefix. Normalize to
    // the hyprctl form so stored addresses compare byte-for-byte.
    Some(if payload.starts_with("0x") {
        payload.to_owned()
    } else {
        format!("0x{payload}")
    })
}

fn handle_watch_focused_window() -> io::Result<()> {
    let socket_path = hyprland::event_socket_path().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Hyprland event socket not found")
    })?;
    let mut backoff = Duration::from_millis(250);
    loop {
        match UnixStream::connect(&socket_path) {
            Ok(stream) => {
                backoff = Duration::from_millis(250);
                let reader = BufReader::new(stream);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let Some(address) = parse_focused_window_address(&line) else {
                        continue;
                    };
                    if let Err(error) = mark_address_read(&address, Utc::now()) {
                        eprintln!("agent-notifier: state update failed: {error}");
                    }
                }
            }
            Err(error) => {
                eprintln!("agent-notifier: hyprland socket unavailable: {error}");
            }
        }
        thread::sleep(backoff);
        backoff = (backoff * 2).min(Duration::from_secs(5));
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
    build_info(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("AGENT_NOTIFIER_COMMIT"),
        env!("AGENT_NOTIFIER_DIRTY"),
        env!("AGENT_NOTIFIER_COMMIT_DATE"),
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
  list-json                Print raw state as JSON
  list-display-json        Print focusable events as display JSON
  version-json             Print build metadata as JSON
  focus-latest             Focus the latest unread event
  focus-id <event-id>      Focus an event by id
  mark-read <event-id>     Mark an event as read
  focused-window-read      Mark events for the focused window as read
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
        CliCommand::Hook => handle_hook("codex").map(|()| 0),
        CliCommand::PiHook => handle_pi_hook().map(|()| 0),
        CliCommand::ClaudeHook => handle_hook("claude").map(|()| 0),
        CliCommand::StatusJson => handle_status_json().map(|()| 0),
        CliCommand::ListJson => {
            print_json(&read_state_or_recover(&state_path()?, Utc::now())?);
            Ok(0)
        }
        CliCommand::ListDisplayJson => {
            let state = read_state_with_focused_window_read(Utc::now())?;
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
        CliCommand::FocusLatest => {
            let state = read_state_or_recover(&state_path()?, Utc::now())?;
            let focusable = focusable_events(&state.events);
            let event = focusable
                .iter()
                .find(|event| event.status == EventStatus::Unread);
            if hyprland::focus_event_source(event) == FocusOutcome::Primary {
                if let Some(id) = event.map(|event| event.id.clone()) {
                    let _ = with_state_update(&state_path()?, Utc::now(), |state| {
                        set_event_status(state, &id, EventStatus::Read)
                    })?;
                }
            }
            Ok(0)
        }
        CliCommand::FocusId(id) => {
            let state = read_state_or_recover(&state_path()?, Utc::now())?;
            let event = state.events.iter().find(|event| event.id == id);
            match hyprland::focus_event_source(event) {
                FocusOutcome::NotFocused => {
                    eprintln!("agent-notifier: could not focus the source window for {id}");
                    Ok(1)
                }
                FocusOutcome::Primary => {
                    let _ = with_state_update(&state_path()?, Utc::now(), |state| {
                        set_event_status(state, &id, EventStatus::Read)
                    })?;
                    Ok(0)
                }
                FocusOutcome::Fallback => Ok(0),
            }
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
            let existing_addresses = hyprland::try_existing_window_addresses()?;
            let _ = with_state_update(&state_path()?, Utc::now(), |state| {
                prune_stale_events(state, &existing_addresses)
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
