use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashSet;
use std::env;
use std::io::{self, BufRead, BufReader, Read};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

mod cli;
mod codex_event;
mod hyprland;
mod pi_event;
mod presentation;
mod process;
mod state;
mod storage;

use cli::CliCommand;
use codex_event::{
    build_stop_event, current_git_branch, parse_codex_stop_input, project_root, random_hex,
};
use pi_event::{build_pi_event, parse_pi_hook_input};
use presentation::{display_state_from_events, event_label, waybar_output, WaybarOutput};
use process::{run_command, run_command_owned, DEFAULT_TIMEOUT};
use state::{
    append_and_trim, clear_read_events, dedupe_events, empty_state, set_event_status,
    set_window_address_read, state_has_unread_for_address, AgentEvent, AgentNotifierState,
    EventStatus, WorkspaceInfo,
};
use storage::{read_state_or_recover, state_path, with_state_update};

const AGENT_CENTER_CLASS: &str = "io.github.bengous.AgentNotifier";
const WAYBAR_SIGNAL: &str = "RTMIN+11";
const UNAVAILABLE_WAYBAR_JSON: &str =
    r#"{"text":"agents !","tooltip":"Agent notifier unavailable","class":"error"}"#;
const UNAVAILABLE_WAYBAR_TOOLTIP: &str = "Agent notifier unavailable";
const WAYBAR_ERROR_CLASS: &str = "error";

fn set_active_window_read(state: AgentNotifierState) -> AgentNotifierState {
    match hyprland::active_window_address() {
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
    focusable_events_for_addresses(events, &hyprland::active_window_addresses())
}

fn focusable_events_for_addresses(
    events: &[AgentEvent],
    active: &HashSet<String>,
) -> Vec<AgentEvent> {
    let focusable = events
        .iter()
        .filter(|event| event_has_live_source(event, active))
        .cloned()
        .collect::<Vec<_>>();
    dedupe_events(focusable)
}

fn event_has_live_source(event: &AgentEvent, active: &HashSet<String>) -> bool {
    event
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.client_address.as_deref())
        .is_some_and(|address| active.contains(address))
}

fn prune_stale_events(
    mut state: AgentNotifierState,
    active_addresses: &HashSet<String>,
) -> AgentNotifierState {
    state
        .events
        .retain(|event| event_has_live_source(event, active_addresses));
    state
}

fn read_state_with_active_window_read(now: DateTime<Utc>) -> io::Result<AgentNotifierState> {
    let path = state_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let current = read_state_or_recover(&path, now)?;
    let next = set_active_window_read(current.clone());
    if next == current {
        return Ok(current);
    }
    with_state_update(&path, now, set_active_window_read)
}

fn format_waybar(state: &AgentNotifierState) -> WaybarOutput {
    waybar_output(&focusable_events(&state.events))
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

fn refresh_waybar() {
    let _ = run_command(
        &["pkill", &format!("-{WAYBAR_SIGNAL}"), "waybar"],
        Duration::from_millis(500),
    );
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
    if hyprland::is_active_source_event(event) {
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
    let waybar = thread::spawn(refresh_waybar);
    let _ = sound.join();
    let _ = notification.join();
    let _ = waybar.join();
    Ok(())
}

/// Resolve the source workspace, retrying once when the address is missing —
/// `hyprctl clients` can race a window that has just been mapped.
fn resolve_source_workspace() -> Option<WorkspaceInfo> {
    let first = hyprland::resolve_current_workspace();
    if first
        .as_ref()
        .is_some_and(|workspace| workspace.client_address.is_some())
    {
        return first;
    }
    thread::sleep(Duration::from_millis(100));
    hyprland::resolve_current_workspace().or(first)
}

fn handle_hook(agent: &str) -> io::Result<()> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let input = parse_codex_stop_input(&raw);
    let cwd = input.cwd.clone().unwrap_or_else(fallback_cwd);
    let now = Utc::now();
    let project_path = project_root(&cwd);
    let branch_name = current_git_branch(&project_path);
    let event = build_stop_event(
        agent,
        &input,
        cwd.clone(),
        project_path,
        branch_name,
        resolve_source_workspace(),
        now,
        &random_hex(4),
    );
    capture_completion_event(&event, now)
}

fn handle_pi_hook() -> io::Result<()> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let input = parse_pi_hook_input(&raw);
    let cwd = input.cwd.clone().unwrap_or_else(fallback_cwd);
    let now = Utc::now();
    let project_path = project_root(&cwd);
    let branch_name = current_git_branch(&project_path);
    let event = build_pi_event(
        &input,
        cwd.clone(),
        project_path,
        branch_name,
        resolve_source_workspace(),
        now,
        &random_hex(4),
    );
    capture_completion_event(&event, now)
}

fn handle_waybar() -> io::Result<()> {
    let state = read_state_with_active_window_read(Utc::now())?;
    let output = std::panic::catch_unwind(|| format_waybar(&state))
        .unwrap_or_else(|_| unavailable_waybar_output());
    print_json(&output);
    Ok(())
}

fn handle_center() -> io::Result<()> {
    // Duplicate suppression belongs to Gtk.Application: a second launch re-activates
    // the primary instance over D-Bus and center.js presents the existing window.
    // A Rust-side preflight cannot see an unmapped Wayland surface and only raced,
    // so Hyprland focus is a post-activation fallback only.

    let center_path = share_dir().join("center.js");
    let cli_command = env::current_exe()?.to_string_lossy().into_owned();
    let _child = Command::new("gjs")
        .arg(center_path)
        .env("AGENT_NOTIFIER_CLI_COMMAND", cli_command)
        .stdout(Stdio::null())
        .spawn()?;
    thread::sleep(Duration::from_millis(300));
    let _ = hyprland::focus_center_window(AGENT_CENTER_CLASS);
    Ok(())
}

fn handle_active_window_read() -> io::Result<()> {
    let Some(address) = hyprland::active_window_address() else {
        return Ok(());
    };
    if mark_address_read(&address, Utc::now())? {
        refresh_waybar();
    }
    Ok(())
}

fn parse_active_window_address(line: &str) -> Option<String> {
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

fn handle_watch_active_window() -> io::Result<()> {
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
                    let Some(address) = parse_active_window_address(&line) else {
                        continue;
                    };
                    match mark_address_read(&address, Utc::now()) {
                        Ok(true) => refresh_waybar(),
                        Ok(false) => {}
                        Err(error) => eprintln!("agent-notifier: state update failed: {error}"),
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
            println!("{UNAVAILABLE_WAYBAR_JSON}");
        }
    }
}

fn unavailable_waybar_output() -> WaybarOutput {
    WaybarOutput {
        text: "agents !".to_owned(),
        tooltip: UNAVAILABLE_WAYBAR_TOOLTIP.to_owned(),
        class: WAYBAR_ERROR_CLASS.to_owned(),
    }
}

fn usage() -> &'static str {
    "Usage: agent-notifier <command>

Commands:
  hook                     Capture a Codex completion from stdin
  pi-hook                  Capture a Pi completion from stdin
  claude-hook              Capture a Claude Code completion from stdin
  waybar                   Print Waybar module JSON
  center                   Open the notification center
  list-json                Print raw state as JSON
  list-display-json        Print focusable events as display JSON
  focus-latest             Focus the latest unread event
  focus-id <event-id>      Focus an event by id
  mark-read <event-id>     Mark an event as read
  active-window-read       Mark events for the active window as read
  watch-active-window      Watch active-window changes
  clear-read               Remove read events
  clear-all                Remove all events
  prune-stale              Remove events whose source window is gone

Options:
  -h, --help               Print help
  -V, --version            Print version"
}

fn run() -> io::Result<i32> {
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
        CliCommand::Waybar => handle_waybar().map(|()| 0),
        CliCommand::Center => handle_center().map(|()| 0),
        CliCommand::ListJson => {
            print_json(&read_state_or_recover(&state_path()?, Utc::now())?);
            Ok(0)
        }
        CliCommand::ListDisplayJson => {
            let state = read_state_with_active_window_read(Utc::now())?;
            print_json(&display_state_from_events(
                state.version,
                focusable_events(&state.events),
            ));
            Ok(0)
        }
        CliCommand::FocusLatest => {
            let state = read_state_or_recover(&state_path()?, Utc::now())?;
            let focusable = focusable_events(&state.events);
            let event = focusable
                .iter()
                .find(|event| event.status == EventStatus::Unread);
            if hyprland::focus_event_source(event) {
                if let Some(id) = event.map(|event| event.id.clone()) {
                    let _ = with_state_update(&state_path()?, Utc::now(), |state| {
                        set_event_status(state, &id, EventStatus::Read)
                    })?;
                    refresh_waybar();
                }
            }
            Ok(0)
        }
        CliCommand::FocusId(id) => {
            let state = read_state_or_recover(&state_path()?, Utc::now())?;
            let event = state.events.iter().find(|event| event.id == id);
            if !hyprland::focus_event_source(event) {
                eprintln!("agent-notifier: could not focus the source window for {id}");
                return Ok(1);
            }
            let _ = with_state_update(&state_path()?, Utc::now(), |state| {
                set_event_status(state, &id, EventStatus::Read)
            })?;
            refresh_waybar();
            Ok(0)
        }
        CliCommand::MarkRead(id) => {
            let _ = with_state_update(&state_path()?, Utc::now(), |state| {
                set_event_status(state, &id, EventStatus::Read)
            })?;
            refresh_waybar();
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
        CliCommand::ActiveWindowRead => handle_active_window_read().map(|()| 0),
        CliCommand::WatchActiveWindow => handle_watch_active_window().map(|()| 0),
        CliCommand::ClearRead => {
            let _ = with_state_update(&state_path()?, Utc::now(), clear_read_events)?;
            refresh_waybar();
            Ok(0)
        }
        CliCommand::ClearAll => {
            let _ = with_state_update(&state_path()?, Utc::now(), |_| empty_state())?;
            refresh_waybar();
            Ok(0)
        }
        CliCommand::PruneStale => {
            let active_addresses = hyprland::try_active_window_addresses()?;
            let _ = with_state_update(&state_path()?, Utc::now(), |state| {
                prune_stale_events(state, &active_addresses)
            })?;
            refresh_waybar();
            Ok(0)
        }
        CliCommand::Unknown => {
            eprintln!("{}", usage());
            Ok(2)
        }
    }
}

/// Exit code used when a hook cannot persist its event.
///
/// A notifier must never fail an agent turn, so a harness only gets a non-zero
/// code once its semantics are *verified* to surface it non-blockingly. Anything
/// unverified stays at 0. Never return 2: Claude Code treats it as a blocking
/// error on Stop hooks.
///
/// Claude Code documents exit 1 as non-blocking and surfaces stderr:
/// <https://code.claude.com/docs/en/hooks>.
fn hook_failure_exit_code(command: &CliCommand) -> i32 {
    match command {
        CliCommand::Hook | CliCommand::PiHook | CliCommand::Waybar => 0,
        _ => 1,
    }
}

fn main() {
    let command = CliCommand::from_env();
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("agent-notifier: {error}");
            if command == CliCommand::Waybar {
                println!("{UNAVAILABLE_WAYBAR_JSON}");
            }
            std::process::exit(hook_failure_exit_code(&command));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex_event::CodexStopInput;
    use crate::pi_event::PiHookInput;
    use crate::state::parse_state;
    use std::fs;
    use tempfile::tempdir;

    fn base_event() -> AgentEvent {
        build_stop_event(
            "codex",
            &CodexStopInput {
                cwd: Some("/repo/dotfiles".to_owned()),
                session_id: Some("session-1".to_owned()),
                session_id_camel: None,
            },
            "/repo/dotfiles".to_owned(),
            "/repo/dotfiles".to_owned(),
            Some("main".to_owned()),
            Some(WorkspaceInfo {
                id: 3,
                name: "3".to_owned(),
                monitor: "DP-3".to_owned(),
                client_pid: 300,
                client_address: None,
                title: "dotfiles | main".to_owned(),
            }),
            DateTime::from_timestamp_millis(1_778_061_600_000).unwrap_or_else(Utc::now),
            "abcd",
        )
    }

    fn base_pi_event(workspace: Option<WorkspaceInfo>) -> AgentEvent {
        build_pi_event(
            &PiHookInput {
                cwd: Some("/repo/dotfiles".to_owned()),
                session_id: None,
                session_id_camel: None,
                session_file: None,
                session_file_camel: Some(
                    "/repo/home/.pi/agent/sessions/pi-session.jsonl".to_owned(),
                ),
                leaf_id: None,
                leaf_id_camel: Some("leaf-1".to_owned()),
            },
            "/repo/dotfiles".to_owned(),
            "/repo/dotfiles".to_owned(),
            Some("main".to_owned()),
            workspace,
            DateTime::from_timestamp_millis(1_778_061_600_000).unwrap_or_else(Utc::now),
            "bcde",
        )
    }

    fn event_with_session(id: &str, session_id: &str) -> AgentEvent {
        AgentEvent {
            id: id.to_owned(),
            session_id: session_id.to_owned(),
            ..base_event()
        }
    }

    fn sessionless_event(id: &str, session_id: &str) -> AgentEvent {
        AgentEvent {
            workspace: None,
            ..event_with_session(id, session_id)
        }
    }

    fn event_with_pid(id: &str, pid: i64) -> AgentEvent {
        let mut base = base_event();
        if let Some(workspace) = &mut base.workspace {
            workspace.client_pid = pid;
        }
        AgentEvent {
            id: id.to_owned(),
            session_id: format!("session-{pid}"),
            ..base
        }
    }

    fn event_with_address(id: &str, pid: i64, address: &str) -> AgentEvent {
        let mut base = base_event();
        if let Some(workspace) = &mut base.workspace {
            workspace.client_pid = pid;
            workspace.client_address = Some(address.to_owned());
        }
        AgentEvent {
            id: id.to_owned(),
            ..base
        }
    }

    fn workspace(event: &AgentEvent) -> Result<WorkspaceInfo, Box<dyn std::error::Error>> {
        event
            .workspace
            .clone()
            .ok_or_else(|| "missing workspace".into())
    }

    #[test]
    fn parses_active_window_addresses_from_socket_lines() {
        assert_eq!(
            parse_active_window_address("activewindowv2>>5934e19c0f30").as_deref(),
            Some("0x5934e19c0f30")
        );
        assert_eq!(
            parse_active_window_address("activewindowv2>>0x5934e19c0f30").as_deref(),
            Some("0x5934e19c0f30")
        );
        assert_eq!(parse_active_window_address("activewindowv2>>"), None);
        assert_eq!(parse_active_window_address("workspace>>3"), None);
    }

    #[test]
    fn parses_codex_stop_json() {
        let input = parse_codex_stop_input(r#"{"cwd":"/repo","session_id":"abc"}"#);
        assert_eq!(input.cwd.as_deref(), Some("/repo"));
        assert_eq!(input.session_id.as_deref(), Some("abc"));
    }

    #[test]
    fn builds_codex_event_shape() {
        let event = base_event();
        assert_eq!(event.id, "1778061600000-abcd");
        assert_eq!(event.agent, "codex");
        assert_eq!(event.kind, "main");
        assert_eq!(event.project_name, "dotfiles");
        assert_eq!(event.branch_name.as_deref(), Some("main"));
        assert_eq!(event.session_id, "session-1");
        assert_eq!(event.status, EventStatus::Unread);
    }

    #[test]
    fn display_state_exposes_exactly_the_keys_the_center_reads() {
        let state = display_state_from_events(1, vec![event_with_address("e", 300, "0xbeef")]);
        let value = serde_json::to_value(&state).unwrap_or(serde_json::Value::Null);
        let event = &value["events"][0];

        for key in [
            "id",
            "agent",
            "status",
            "projectName",
            "createdAt",
            "displayLabel",
            "displayCreatedAt",
            "workspace",
        ] {
            assert!(!event[key].is_null(), "missing key: {key}");
        }
        assert!(!value["version"].is_null());
        assert!(!event["workspace"]["name"].is_null());
    }

    #[test]
    fn builds_claude_event_shape() -> Result<(), Box<dyn std::error::Error>> {
        let event = build_stop_event(
            "claude",
            &CodexStopInput {
                cwd: Some("/repo/dotfiles".to_owned()),
                session_id: Some("claude-session-1".to_owned()),
                session_id_camel: None,
            },
            "/repo/dotfiles".to_owned(),
            "/repo/dotfiles".to_owned(),
            Some("main".to_owned()),
            Some(workspace(&base_event())?),
            DateTime::from_timestamp_millis(1_778_061_600_000).unwrap_or_else(Utc::now),
            "abcd",
        );
        assert_eq!(event.agent, "claude");
        assert_eq!(event.kind, "main");
        assert_eq!(event.session_id, "claude-session-1");
        assert_eq!(agent_display_name(&event.agent), "Claude");
        Ok(())
    }

    #[test]
    fn parses_pi_hook_json() {
        let input = parse_pi_hook_input(
            r#"{"cwd":"/repo","sessionFile":"/repo/home/.pi/session.jsonl","leafId":"leaf-1"}"#,
        );
        assert_eq!(input.cwd.as_deref(), Some("/repo"));
        assert_eq!(
            input.session_file_camel.as_deref(),
            Some("/repo/home/.pi/session.jsonl")
        );
        assert_eq!(input.leaf_id_camel.as_deref(), Some("leaf-1"));
    }

    #[test]
    fn stores_pi_events_as_main_agent_events() -> Result<(), Box<dyn std::error::Error>> {
        let event = base_pi_event(Some(workspace(&base_event())?));
        let state = append_and_trim(empty_state(), event);

        assert_eq!(
            state.events.first().map(|event| event.agent.as_str()),
            Some("pi")
        );
        assert_eq!(
            state.events.first().map(|event| event.kind.as_str()),
            Some("main")
        );
        assert_eq!(
            state.events.first().map(|event| event.session_id.as_str()),
            Some("/repo/home/.pi/agent/sessions/pi-session.jsonl")
        );
        Ok(())
    }

    #[test]
    fn rejects_invalid_state_shape() {
        let result = state::parse_state(r#"{"version":1,"events":[{"id":"missing-fields"}]}"#);
        assert!(result.is_err());
    }

    #[test]
    fn appends_newest_first_and_trims() {
        let mut state = empty_state();
        for index in 0..55 {
            state = append_and_trim(
                state,
                AgentEvent {
                    id: format!("event-{index}"),
                    session_id: format!("session-{index}"),
                    workspace: Some(WorkspaceInfo {
                        client_pid: i64::from(index),
                        ..workspace(&base_event()).unwrap_or_else(|_| WorkspaceInfo {
                            id: 3,
                            name: "3".to_owned(),
                            monitor: "DP-3".to_owned(),
                            client_pid: i64::from(index),
                            client_address: None,
                            title: "dotfiles | main".to_owned(),
                        })
                    }),
                    ..base_event()
                },
            );
        }
        assert_eq!(state.events.len(), 50);
        assert_eq!(
            state.events.first().map(|event| event.id.as_str()),
            Some("event-54")
        );
        assert_eq!(
            state.events.last().map(|event| event.id.as_str()),
            Some("event-5")
        );
    }

    #[test]
    fn dedupes_sessionless_events_by_session() {
        let state = AgentNotifierState {
            version: 1,
            events: vec![sessionless_event("old", "session-1")],
        };
        let state = append_and_trim(state, sessionless_event("new", "session-1"));

        assert_eq!(state.events.len(), 1);
        assert_eq!(
            state.events.first().map(|event| event.id.as_str()),
            Some("new")
        );
    }

    #[test]
    fn client_pid_takes_precedence_over_session_id() {
        let state = AgentNotifierState {
            version: 1,
            events: vec![event_with_session("old", "session-1")],
        };
        let state = append_and_trim(state, event_with_session("new", "session-2"));

        assert_eq!(state.events.len(), 1);
        assert_eq!(
            state.events.first().map(|event| event.id.as_str()),
            Some("new")
        );
    }

    #[test]
    fn one_session_across_two_windows_stays_separate() {
        let mut second = event_with_session("second", "session-1");
        if let Some(workspace) = &mut second.workspace {
            workspace.client_pid = 301;
        }
        let state = AgentNotifierState {
            version: 1,
            events: vec![event_with_session("first", "session-1")],
        };
        let state = append_and_trim(state, second);

        assert_eq!(state.events.len(), 2);
    }

    #[test]
    fn keeps_unknown_session_events_separate() {
        let state = AgentNotifierState {
            version: 1,
            events: vec![
                AgentEvent {
                    workspace: None,
                    ..event_with_session("first", "unknown")
                },
                AgentEvent {
                    workspace: None,
                    ..event_with_session("second", "unknown")
                },
            ],
        };

        assert_eq!(dedupe_events(state.events).len(), 2);
    }

    #[test]
    fn backs_up_corrupted_state() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let path = dir.path().join("events.json");
        fs::write(&path, "{bad json")?;
        let state = with_state_update(
            &path,
            DateTime::from_timestamp_millis(1_778_061_600_000).unwrap_or_else(Utc::now),
            |state| append_and_trim(state, base_event()),
        )?;
        assert_eq!(state.events.len(), 1);
        let backups = fs::read_dir(dir.path())?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("events.json.corrupt-")
            })
            .count();
        assert_eq!(backups, 1);
        Ok(())
    }

    #[test]
    fn uses_cleaned_hyprland_title_with_branch_fallback() -> Result<(), Box<dyn std::error::Error>>
    {
        assert_eq!(
            presentation::clean_workspace_title("⠴ dotfiles | main"),
            "dotfiles | main"
        );
        assert_eq!(event_label(&base_event()), "dotfiles | main");
        let fallback = AgentEvent {
            workspace: Some(WorkspaceInfo {
                title: String::new(),
                ..workspace(&base_event())?
            }),
            ..base_event()
        };
        assert_eq!(event_label(&fallback), "dotfiles | main");
        assert_eq!(
            event_label(&AgentEvent {
                workspace: None,
                ..base_event()
            }),
            "dotfiles | main"
        );
        assert_eq!(
            event_label(&AgentEvent {
                branch_name: None,
                workspace: None,
                ..base_event()
            }),
            "dotfiles"
        );
        Ok(())
    }

    #[test]
    fn formats_agent_button_label() {
        assert_eq!(presentation::format_agent_button(0), "agents");
        assert_eq!(presentation::format_agent_button(3), "agents 󰂚 3");
    }

    #[test]
    fn unavailable_waybar_output_is_visible() {
        let output = unavailable_waybar_output();
        assert_eq!(output.text, "agents !");
        assert_eq!(output.class, "error");
    }

    #[test]
    fn hook_failures_never_block_an_agent_turn() {
        for command in [
            CliCommand::Hook,
            CliCommand::PiHook,
            CliCommand::ClaudeHook,
            CliCommand::Waybar,
        ] {
            assert_ne!(hook_failure_exit_code(&command), 2);
        }
    }

    #[test]
    fn hook_failure_exit_codes_follow_verified_harness_policy() {
        assert_eq!(hook_failure_exit_code(&CliCommand::ClaudeHook), 1);
        for command in [CliCommand::Hook, CliCommand::PiHook, CliCommand::Waybar] {
            assert_eq!(hook_failure_exit_code(&command), 0);
        }
    }

    #[test]
    fn counts_unread_events_in_waybar_label() {
        let event = event_with_pid("event-1", 1);
        let other = event_with_pid("event-2", 2);
        let read = AgentEvent {
            id: "read".to_owned(),
            status: EventStatus::Read,
            ..event.clone()
        };
        let state = AgentNotifierState {
            version: 1,
            events: vec![event.clone(), read, other],
        };
        let output = waybar_output(&dedupe_events(state.events));
        assert_eq!(output.text, "agents 󰂚 2");
        assert_eq!(output.class, "unread");
    }

    #[test]
    fn marking_duplicate_session_read_updates_all_copies() {
        let state = AgentNotifierState {
            version: 1,
            events: vec![
                sessionless_event("new", "session-1"),
                sessionless_event("old", "session-1"),
            ],
        };
        let state = set_event_status(state, "new", EventStatus::Read);

        assert!(state
            .events
            .iter()
            .all(|event| event.status == EventStatus::Read));
        assert_eq!(waybar_output(&dedupe_events(state.events)).text, "agents");
    }

    #[test]
    fn marking_window_address_read_updates_counter() {
        let state = AgentNotifierState {
            version: 1,
            events: vec![
                event_with_address("focused", 1, "0xfocused"),
                event_with_address("other", 2, "0xother"),
            ],
        };
        let state = set_window_address_read(state, "0xfocused");

        assert_eq!(
            waybar_output(&dedupe_events(state.events)).text,
            "agents 󰂚 1"
        );
    }

    #[test]
    fn captures_only_events_with_source_addresses() {
        assert!(!should_capture_event(&AgentEvent {
            workspace: None,
            ..base_event()
        }));
        assert!(!should_capture_event(&base_pi_event(None)));
        assert!(!should_capture_event(&base_event()));
        assert!(should_capture_event(&event_with_address(
            "addressed",
            300,
            "0xbeef"
        )));
    }

    #[test]
    fn emits_waybar_json_shape() -> Result<(), Box<dyn std::error::Error>> {
        let state = AgentNotifierState {
            version: 1,
            events: vec![AgentEvent {
                project_name: "quote\"project".to_owned(),
                workspace: Some(WorkspaceInfo {
                    title: "line\nbreak".to_owned(),
                    ..workspace(&base_event())?
                }),
                ..base_event()
            }],
        };
        let output = waybar_output(&dedupe_events(state.events));
        assert_eq!(output.text, "agents 󰂚 1");
        assert_eq!(output.tooltip, "line\nbreak May 6, 2026 10:00 AM UTC");
        assert_eq!(output.class, "unread");
        Ok(())
    }

    #[test]
    fn ignores_read_events() {
        let mut event = base_event();
        event.status = EventStatus::Read;
        let output = waybar_output(&dedupe_events(vec![event]));
        assert_eq!(
            output,
            WaybarOutput {
                text: "agents".to_owned(),
                tooltip: "No agent completions".to_owned(),
                class: "empty".to_owned(),
            }
        );
    }

    #[test]
    fn a_recycled_pid_does_not_resurrect_a_dead_event() {
        let dead = event_with_address("dead", 300, "0xdead");
        let mut live_addresses = HashSet::new();
        // The window that owned pid 300 is gone; an unrelated live window now has it.
        live_addresses.insert("0xbeef".to_owned());

        let focusable = focusable_events_for_addresses(&[dead], &live_addresses);

        assert!(focusable.is_empty());
    }

    #[test]
    fn an_event_is_focusable_while_its_address_is_live() {
        let alive = event_with_address("alive", 300, "0xbeef");
        let mut live_addresses = HashSet::new();
        live_addresses.insert("0xbeef".to_owned());

        assert_eq!(
            focusable_events_for_addresses(&[alive], &live_addresses).len(),
            1
        );
    }

    #[test]
    fn legacy_events_without_an_address_are_not_focusable() {
        let legacy = event_with_pid("legacy", 300);
        let mut live_addresses = HashSet::new();
        live_addresses.insert("0xbeef".to_owned());

        assert!(focusable_events_for_addresses(&[legacy], &live_addresses).is_empty());
    }

    #[test]
    fn parses_v1_state_without_client_address() -> Result<(), Box<dyn std::error::Error>> {
        let raw = r#"{"version":1,"events":[{"id":"e","agent":"claude","kind":"main",
            "projectName":"p","projectPath":"/repo/dotfiles","cwd":"/repo/dotfiles",
            "sessionId":"s","createdAt":"2026-07-26T08:00:00.000Z",
            "workspace":{"id":1,"name":"1","monitor":"DP-3","clientPid":42,"title":"t"},
            "status":"unread"}]}"#;
        let state = parse_state(raw)?;
        assert_eq!(state.events.len(), 1);
        Ok(())
    }

    #[test]
    fn excludes_stored_events_when_no_live_hyprland_addresses_exist() {
        let events = vec![
            AgentEvent {
                workspace: None,
                ..event_with_session("stale-session-only", "session-stale")
            },
            event_with_address("stale-window", 42, "0xstale"),
        ];

        let focusable = focusable_events_for_addresses(&events, &HashSet::new());

        assert!(focusable.is_empty());
        assert_eq!(waybar_output(&focusable).text, "agents");
    }

    #[test]
    fn includes_only_events_matching_live_hyprland_addresses() {
        let events = vec![
            event_with_address("live-window", 42, "0xlive"),
            event_with_address("stale-window", 7, "0xstale"),
            AgentEvent {
                workspace: None,
                ..event_with_session("session-only", "session-only")
            },
        ];
        let active_addresses = HashSet::from(["0xlive".to_owned()]);

        let focusable = focusable_events_for_addresses(&events, &active_addresses);

        assert_eq!(focusable.len(), 1);
        assert_eq!(
            focusable.first().map(|event| event.id.as_str()),
            Some("live-window")
        );
        assert_eq!(waybar_output(&focusable).text, "agents 󰂚 1");
    }

    #[test]
    fn prunes_events_without_live_hyprland_addresses() {
        let state = AgentNotifierState {
            version: 1,
            events: vec![
                event_with_address("live-window", 42, "0xlive"),
                event_with_address("stale-window", 7, "0xstale"),
                AgentEvent {
                    workspace: None,
                    ..event_with_session("session-only", "session-only")
                },
            ],
        };
        let active_addresses = HashSet::from(["0xlive".to_owned()]);

        let pruned = prune_stale_events(state, &active_addresses);

        assert_eq!(pruned.events.len(), 1);
        assert_eq!(
            pruned.events.first().map(|event| event.id.as_str()),
            Some("live-window")
        );
    }
}
