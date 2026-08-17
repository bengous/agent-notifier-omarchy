use super::*;
use crate::pi_event::PiHookInput;
use crate::state::parse_state;
use crate::{hook_failure_exit_code, presentation, state};
use std::ffi::OsStr;
use std::fs;
use tempfile::tempdir;

fn base_event() -> AgentEvent {
    build_stop_event(
        "codex",
        &StopHookInput {
            cwd: Some("/repo/dotfiles".to_owned()),
            session_id: Some("session-1".to_owned()),
            session_id_camel: None,
            transcript_path: None,
        },
        "/repo/dotfiles".to_owned(),
        "/repo/dotfiles".to_owned(),
        "/repo/dotfiles".to_owned(),
        Some("main".to_owned()),
        Some(SourceWindow {
            id: 3,
            name: "3".to_owned(),
            monitor: "DP-3".to_owned(),
            client_pid: 300,
            client_address: None,
            client_addresses: Vec::new(),
            source_process: None,
            title: "dotfiles | main".to_owned(),
            extra: serde_json::Map::new(),
        }),
        DateTime::from_timestamp_millis(1_778_061_600_000).unwrap_or_else(Utc::now),
        "abcd",
    )
}

fn base_pi_event(workspace: Option<SourceWindow>) -> AgentEvent {
    build_pi_event(
        &PiHookInput {
            cwd: Some("/repo/dotfiles".to_owned()),
            session_id: None,
            session_id_camel: None,
            session_file: None,
            session_file_camel: Some("/repo/home/.pi/agent/sessions/pi-session.jsonl".to_owned()),
            leaf_id: None,
            leaf_id_camel: Some("leaf-1".to_owned()),
        },
        "/repo/dotfiles".to_owned(),
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
    let mut base = event_with_pid(id, pid);
    if let Some(workspace) = &mut base.workspace {
        workspace.client_address = Some(address.to_owned());
    }
    base
}

fn event_with_candidates(id: &str, pid: i64, addresses: &[&str]) -> AgentEvent {
    let mut base = event_with_pid(id, pid);
    if let Some(workspace) = &mut base.workspace {
        workspace.client_address = addresses.first().map(|address| (*address).to_owned());
        workspace.client_addresses = addresses
            .iter()
            .map(|address| (*address).to_owned())
            .collect();
    }
    base
}

fn event_in_project(
    id: &str,
    pid: i64,
    project_key: Option<&str>,
    project_path: &str,
) -> AgentEvent {
    AgentEvent {
        project_key: project_key.map(str::to_owned),
        project_name: Path::new(project_path)
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or(project_path)
            .to_owned(),
        project_path: project_path.to_owned(),
        ..event_with_pid(id, pid)
    }
}

fn displayed_projects(events: Vec<AgentEvent>) -> Vec<(String, String)> {
    display_state_from_events(1, events)
        .events
        .into_iter()
        .map(|row| (row.event.id, row.display_project))
        .collect()
}

fn workspace(event: &AgentEvent) -> Result<SourceWindow, Box<dyn std::error::Error>> {
    event
        .workspace
        .clone()
        .ok_or_else(|| "missing workspace".into())
}

fn state_of(events: Vec<AgentEvent>) -> AgentNotifierState {
    AgentNotifierState {
        events,
        ..empty_state()
    }
}

#[test]
fn parses_focused_window_addresses_from_socket_lines() {
    assert_eq!(
        parse_focused_window_address("activewindowv2>>5934e19c0f30").as_deref(),
        Some("0x5934e19c0f30")
    );
    assert_eq!(
        parse_focused_window_address("activewindowv2>>0x5934e19c0f30").as_deref(),
        Some("0x5934e19c0f30")
    );
    assert_eq!(parse_focused_window_address("activewindowv2>>"), None);
    assert_eq!(parse_focused_window_address("workspace>>3"), None);
}

#[test]
fn parses_stop_hook_json() {
    let input = parse_stop_hook_input(r#"{"cwd":"/repo","session_id":"abc"}"#);
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
    assert_eq!(event.project_key.as_deref(), Some("/repo/dotfiles"));
    assert_eq!(event.branch_name.as_deref(), Some("main"));
    assert_eq!(event.session_id, "session-1");
    assert_eq!(event.status, EventStatus::Unread);
}

#[test]
fn display_state_exposes_exactly_the_keys_the_widget_reads() {
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
        "displayProject",
        "workspace",
    ] {
        assert!(!event[key].is_null(), "missing key: {key}");
    }
    assert!(!value["version"].is_null());
    assert!(!event["workspace"]["name"].is_null());
}

fn build_info_fixture(state_path: Option<&Path>) -> presentation::BuildInfo {
    presentation::build_info(
        "agent-notifier",
        "0.3.0",
        "abc1234",
        "true",
        "2026-08-16T10:00:00+00:00",
        state_path,
    )
}

#[test]
fn version_json_exposes_exactly_the_keys_the_widget_reads() {
    let value = serde_json::to_value(build_info_fixture(None)).unwrap_or(serde_json::Value::Null);

    for key in ["name", "version", "commit", "dirty", "commitDate"] {
        assert!(!value[key].is_null(), "missing key: {key}");
    }
    assert_eq!(value["dirty"], serde_json::Value::Bool(true));
}

#[test]
fn a_state_path_serializes_additively() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("/state/agent-notifier/events.json");
    let with_path = serde_json::to_value(build_info_fixture(Some(path)))?;
    let mut v1 = serde_json::to_value(build_info_fixture(None))?;

    assert_eq!(with_path["statePath"], "/state/agent-notifier/events.json");
    v1["statePath"] = with_path["statePath"].clone();
    assert_eq!(with_path, v1);
    Ok(())
}

#[test]
fn version_json_without_a_state_path_keeps_the_v1_keys() -> Result<(), Box<dyn std::error::Error>> {
    let value = serde_json::to_value(build_info_fixture(None))?;
    let object = value.as_object().ok_or("version-json is not an object")?;
    let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();

    assert_eq!(keys, ["commit", "commitDate", "dirty", "name", "version"]);
    Ok(())
}

#[test]
fn build_info_reports_dirty_only_when_the_tree_is_modified() {
    let dirty = |flag: &str| presentation::build_info("n", "v", "c", flag, "d", None).dirty;

    assert!(dirty("true"));
    assert!(!dirty("false"));
    assert!(!dirty(""));
    assert!(!dirty("garbage"));

    let fallback = presentation::build_info("n", "v", "unknown", "false", "unknown", None);
    assert_eq!(fallback.commit, "unknown");
    assert_eq!(fallback.commit_date, "unknown");
}

#[test]
fn the_build_script_always_supplies_commit_metadata() {
    assert!(!env!("AGENT_NOTIFIER_COMMIT").is_empty());
    assert!(!env!("AGENT_NOTIFIER_DIRTY").is_empty());
    assert!(!env!("AGENT_NOTIFIER_COMMIT_DATE").is_empty());
}

#[test]
fn groups_events_by_project_with_the_newest_group_first() {
    let rows = displayed_projects(vec![
        event_in_project("alpha-new", 1, Some("/repo/alpha"), "/repo/alpha"),
        event_in_project("beta", 2, Some("/repo/beta"), "/repo/beta"),
        event_in_project("alpha-old", 3, Some("/repo/alpha"), "/repo/alpha"),
    ]);

    assert_eq!(
        rows,
        vec![
            ("alpha-new".to_owned(), "alpha".to_owned()),
            ("alpha-old".to_owned(), "alpha".to_owned()),
            ("beta".to_owned(), "beta".to_owned()),
        ]
    );
}

#[test]
fn two_worktrees_of_one_repository_share_the_main_repository_group() {
    let rows = displayed_projects(vec![
        event_in_project("worktree", 1, Some("/repo/alpha"), "/repo/alpha-feature"),
        event_in_project("beta", 2, Some("/repo/beta"), "/repo/beta"),
        event_in_project("main-worktree", 3, Some("/repo/alpha"), "/repo/alpha"),
    ]);

    assert_eq!(
        rows,
        vec![
            ("worktree".to_owned(), "alpha".to_owned()),
            ("main-worktree".to_owned(), "alpha".to_owned()),
            ("beta".to_owned(), "beta".to_owned()),
        ]
    );
}

#[test]
fn events_without_a_project_key_group_by_project_path() {
    let rows = displayed_projects(vec![
        event_in_project("alpha-new", 1, None, "/repo/alpha"),
        event_in_project("beta", 2, None, "/repo/beta"),
        event_in_project("alpha-old", 3, None, "/repo/alpha"),
    ]);

    assert_eq!(
        rows,
        vec![
            ("alpha-new".to_owned(), "alpha".to_owned()),
            ("alpha-old".to_owned(), "alpha".to_owned()),
            ("beta".to_owned(), "beta".to_owned()),
        ]
    );
}

#[test]
fn projects_sharing_a_directory_name_get_their_parent_directory() {
    let rows = displayed_projects(vec![
        event_in_project("work", 1, Some("/work/dotfiles"), "/work/dotfiles"),
        event_in_project(
            "personal",
            2,
            Some("/personal/dotfiles"),
            "/personal/dotfiles",
        ),
        event_in_project("alpha", 3, Some("/work/alpha"), "/work/alpha"),
    ]);

    assert_eq!(
        rows,
        vec![
            ("work".to_owned(), "dotfiles — work".to_owned()),
            ("personal".to_owned(), "dotfiles — personal".to_owned()),
            ("alpha".to_owned(), "alpha".to_owned()),
        ]
    );
}

#[test]
fn a_keyless_project_label_falls_back_to_the_project_name() {
    let rows = displayed_projects(vec![event_in_project(
        "rootless",
        1,
        Some("/"),
        "/repo/dotfiles",
    )]);

    assert_eq!(rows, vec![("rootless".to_owned(), "dotfiles".to_owned())]);
}

#[test]
fn parses_v1_state_without_a_project_key() -> Result<(), Box<dyn std::error::Error>> {
    let raw = r#"{"version":1,"events":[{"id":"e","agent":"claude","kind":"main",
        "projectName":"p","projectPath":"/repo/dotfiles","cwd":"/repo/dotfiles",
        "sessionId":"s","createdAt":"2026-07-26T08:00:00.000Z",
        "workspace":{"id":1,"name":"1","monitor":"DP-3","clientPid":42,"title":"t"},
        "status":"unread"}]}"#;
    let state = parse_state(raw)?;

    assert_eq!(
        state
            .events
            .first()
            .and_then(|event| event.project_key.clone()),
        None
    );
    Ok(())
}

#[test]
fn builds_claude_event_shape() -> Result<(), Box<dyn std::error::Error>> {
    let event = build_stop_event(
        "claude",
        &StopHookInput {
            cwd: Some("/repo/dotfiles".to_owned()),
            session_id: Some("claude-session-1".to_owned()),
            session_id_camel: None,
            transcript_path: None,
        },
        "/repo/dotfiles".to_owned(),
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
                workspace: Some(SourceWindow {
                    client_pid: i64::from(index),
                    ..workspace(&base_event()).unwrap_or_else(|_| SourceWindow {
                        id: 3,
                        name: "3".to_owned(),
                        monitor: "DP-3".to_owned(),
                        client_pid: i64::from(index),
                        client_address: None,
                        client_addresses: Vec::new(),
                        source_process: None,
                        title: "dotfiles | main".to_owned(),
                        extra: serde_json::Map::new(),
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
    let state = state_of(vec![sessionless_event("old", "session-1")]);
    let state = append_and_trim(state, sessionless_event("new", "session-1"));

    assert_eq!(state.events.len(), 1);
    assert_eq!(
        state.events.first().map(|event| event.id.as_str()),
        Some("new")
    );
}

#[test]
fn two_sessions_in_one_window_stay_separate() {
    let state = state_of(vec![event_with_session("old", "session-1")]);
    let state = append_and_trim(state, event_with_session("new", "session-2"));

    assert_eq!(state.events.len(), 2);
}

#[test]
fn two_sessions_sharing_one_terminal_pid_stay_separate() {
    let first = AgentEvent {
        session_id: "session-1".to_owned(),
        ..event_with_address("first", 4682, "0x55e2cd8284a0")
    };
    let second = AgentEvent {
        session_id: "session-2".to_owned(),
        ..event_with_address("second", 4682, "0x55e2cd756b00")
    };
    let state = state_of(vec![first]);
    let state = append_and_trim(state, second);

    assert_eq!(state.events.len(), 2);
    assert_eq!(status_output(&state.events).text, "agents 󰂚 2");
}

#[test]
fn one_session_moved_to_another_window_merges_into_the_newest() {
    let mut second = event_with_session("second", "session-1");
    if let Some(workspace) = &mut second.workspace {
        workspace.client_pid = 301;
    }
    let state = state_of(vec![event_with_session("first", "session-1")]);
    let state = append_and_trim(state, second);

    assert_eq!(state.events.len(), 1);
    assert_eq!(
        state.events.first().map(|event| event.id.as_str()),
        Some("second")
    );
}

#[test]
fn keeps_unknown_session_events_separate() {
    let state = state_of(vec![
        AgentEvent {
            workspace: None,
            ..event_with_session("first", "unknown")
        },
        AgentEvent {
            workspace: None,
            ..event_with_session("second", "unknown")
        },
    ]);

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

fn fixture_clock() -> Result<DateTime<Utc>, Box<dyn std::error::Error>> {
    DateTime::from_timestamp_millis(1_778_061_600_000).ok_or_else(|| "invalid fixture clock".into())
}

fn stored_event_ids(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(parse_state(&fs::read_to_string(path)?)?
        .events
        .into_iter()
        .map(|event| event.id)
        .collect())
}

#[test]
fn mark_read_persists_the_read_status() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("events.json");
    let now = fixture_clock()?;
    with_state_update(&path, now, |state| {
        append_and_trim(state, event_with_address("read-me", 300, "0xbeef"))
    })?;

    with_state_update(&path, now, |state| {
        set_event_status(state, "read-me", EventStatus::Read)
    })?;

    let stored = parse_state(&fs::read_to_string(&path)?)?;
    assert_eq!(
        stored.events.first().map(|event| event.status),
        Some(EventStatus::Read)
    );
    Ok(())
}

#[test]
fn prune_stale_keeps_only_events_with_an_existing_source_window(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("events.json");
    let now = fixture_clock()?;
    with_state_update(&path, now, |state| {
        let state = append_and_trim(state, event_with_address("gone", 7, "0xstale"));
        append_and_trim(state, event_with_address("here", 42, "0xlive"))
    })?;
    let existing_addresses = HashSet::from(["0xlive".to_owned()]);

    with_state_update(&path, now, |state| {
        prune_stale_events(state, &existing_addresses)
    })?;

    assert_eq!(stored_event_ids(&path)?, ["here"]);
    Ok(())
}

#[test]
fn uses_cleaned_hyprland_title_with_branch_fallback() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        presentation::clean_window_title("⠴ dotfiles | main"),
        "dotfiles | main"
    );
    assert_eq!(event_label(&base_event()), "dotfiles | main");
    let fallback = AgentEvent {
        workspace: Some(SourceWindow {
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
fn unavailable_status_output_is_visible() {
    let output = unavailable_status_output();
    assert_eq!(output.text, "agents !");
    assert_eq!(output.class, "error");
}

#[test]
fn hook_failures_never_block_an_agent_turn() {
    for command in [
        CliCommand::Hook,
        CliCommand::PiHook,
        CliCommand::ClaudeHook,
        CliCommand::StatusJson,
    ] {
        assert_ne!(hook_failure_exit_code(&command), 2);
    }
}

#[test]
fn hook_failure_exit_codes_follow_verified_harness_policy() {
    assert_eq!(hook_failure_exit_code(&CliCommand::ClaudeHook), 1);
    for command in [CliCommand::Hook, CliCommand::PiHook, CliCommand::StatusJson] {
        assert_eq!(hook_failure_exit_code(&command), 0);
    }
}

#[test]
fn counts_unread_events_in_status_label() {
    let event = event_with_pid("event-1", 1);
    let other = event_with_pid("event-2", 2);
    let read = AgentEvent {
        id: "read".to_owned(),
        status: EventStatus::Read,
        ..event.clone()
    };
    let state = state_of(vec![event.clone(), read, other]);
    let output = status_output(&dedupe_events(state.events));
    assert_eq!(output.text, "agents 󰂚 2");
    assert_eq!(output.class, "unread");
}

#[test]
fn marking_duplicate_session_read_updates_all_copies() {
    let state = state_of(vec![
        sessionless_event("new", "session-1"),
        sessionless_event("old", "session-1"),
    ]);
    let state = set_event_status(state, "new", EventStatus::Read);

    assert!(state
        .events
        .iter()
        .all(|event| event.status == EventStatus::Read));
    assert_eq!(status_output(&dedupe_events(state.events)).text, "agents");
}

#[test]
fn marking_window_address_read_updates_counter() {
    let state = state_of(vec![
        event_with_address("focused", 1, "0xfocused"),
        event_with_address("other", 2, "0xother"),
    ]);
    let state = set_window_address_read(state, "0xfocused");

    assert_eq!(
        status_output(&dedupe_events(state.events)).text,
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
fn emits_status_json_shape() -> Result<(), Box<dyn std::error::Error>> {
    let state = state_of(vec![AgentEvent {
        project_name: "quote\"project".to_owned(),
        workspace: Some(SourceWindow {
            title: "line\nbreak".to_owned(),
            ..workspace(&base_event())?
        }),
        ..base_event()
    }]);
    let output = status_output(&dedupe_events(state.events));
    assert_eq!(output.text, "agents 󰂚 1");
    assert_eq!(output.tooltip, "line\nbreak May 6, 2026 10:00 AM UTC");
    assert_eq!(output.class, "unread");
    Ok(())
}

#[test]
fn ignores_read_events() {
    let mut event = base_event();
    event.status = EventStatus::Read;
    let output = status_output(&dedupe_events(vec![event]));
    assert_eq!(
        output,
        StatusOutput {
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
fn parses_v1_state_without_session_title() -> Result<(), Box<dyn std::error::Error>> {
    let raw = r#"{"version":1,"events":[{"id":"e","agent":"claude","kind":"main",
        "projectName":"p","projectPath":"/repo/dotfiles","cwd":"/repo/dotfiles",
        "sessionId":"s","createdAt":"2026-07-26T08:00:00.000Z",
        "workspace":{"id":1,"name":"1","monitor":"DP-3","clientPid":42,"title":"t"},
        "status":"unread"}]}"#;
    let state = parse_state(raw)?;

    assert_eq!(
        state
            .events
            .first()
            .and_then(|event| event.session_title.clone()),
        None
    );
    assert_eq!(state.events.first().map(event_label).as_deref(), Some("t"));
    Ok(())
}

#[test]
fn a_session_title_outranks_the_window_title() {
    let titled = AgentEvent {
        session_title: Some("Label events by session".to_owned()),
        ..base_event()
    };

    assert_eq!(event_label(&titled), "Label events by session");
    assert_eq!(
        event_label(&AgentEvent {
            session_title: Some("   ".to_owned()),
            ..base_event()
        }),
        "dotfiles | main"
    );
}

#[test]
fn a_session_title_is_omitted_from_stored_state_when_absent() {
    let json = serde_json::to_string(&base_event()).unwrap_or_default();

    assert!(!json.contains("sessionTitle"));
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
    assert_eq!(status_output(&focusable).text, "agents");
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
    let existing_addresses = HashSet::from(["0xlive".to_owned()]);

    let focusable = focusable_events_for_addresses(&events, &existing_addresses);

    assert_eq!(focusable.len(), 1);
    assert_eq!(
        focusable.first().map(|event| event.id.as_str()),
        Some("live-window")
    );
    assert_eq!(status_output(&focusable).text, "agents 󰂚 1");
}

fn event_with_source_process(id: &str, process: state::ProcessRef) -> AgentEvent {
    let mut base = event_with_address(id, 4682, "0xguess");
    if let Some(workspace) = &mut base.workspace {
        workspace.source_process = Some(process);
    }
    base
}

fn own_process_ref() -> Result<state::ProcessRef, Box<dyn std::error::Error>> {
    crate::hyprland::process_ref(i64::from(std::process::id()))
        .ok_or_else(|| "no process ref for the test process".into())
}

#[test]
fn an_event_with_a_live_source_process_stays_focusable_without_live_addresses(
) -> Result<(), Box<dyn std::error::Error>> {
    let event = event_with_source_process("session-alive", own_process_ref()?);

    let focusable = focusable_events_for_addresses(&[event], &HashSet::new());

    assert_eq!(focusable.len(), 1);
    Ok(())
}

#[test]
fn an_event_with_a_dead_source_process_is_pruned_despite_live_candidates(
) -> Result<(), Box<dyn std::error::Error>> {
    let own = own_process_ref()?;
    let dead = state::ProcessRef {
        start_time: own.start_time.wrapping_add(1),
        ..own
    };
    let state = state_of(vec![event_with_source_process("session-dead", dead)]);
    let existing_addresses = HashSet::from(["0xguess".to_owned()]);

    assert!(prune_stale_events(state, &existing_addresses)
        .events
        .is_empty());
    Ok(())
}

#[test]
fn a_source_process_serializes_additively() -> Result<(), Box<dyn std::error::Error>> {
    let legacy_json = serde_json::to_string(&event_with_address("legacy", 1, "0xbeef"))?;
    assert!(!legacy_json.contains("sourceProcess"));

    let process = state::ProcessRef {
        pid: 146_082,
        start_time: 737_679,
    };
    let value = serde_json::to_value(event_with_source_process("anchored", process))?;
    assert_eq!(value["workspace"]["sourceProcess"]["pid"], 146_082);
    assert_eq!(value["workspace"]["sourceProcess"]["startTime"], 737_679);

    let raw = serde_json::to_string(&state_of(vec![event_with_source_process(
        "anchored", process,
    )]))?;
    let parsed = parse_state(&raw)?;
    assert_eq!(
        parsed
            .events
            .first()
            .and_then(|event| event.workspace.as_ref())
            .and_then(|workspace| workspace.source_process),
        Some(process)
    );
    Ok(())
}

#[test]
fn an_event_with_any_live_candidate_stays_focusable() {
    let event = event_with_candidates("guessed", 4682, &["0xguess", "0xother"]);
    let existing_addresses = HashSet::from(["0xother".to_owned()]);

    assert_eq!(
        focusable_events_for_addresses(&[event], &existing_addresses).len(),
        1
    );
}

#[test]
fn an_event_with_no_live_candidate_is_pruned() {
    let state = state_of(vec![event_with_candidates(
        "guessed",
        4682,
        &["0xguess", "0xother"],
    )]);
    let existing_addresses = HashSet::from(["0xelse".to_owned()]);

    assert!(prune_stale_events(state, &existing_addresses)
        .events
        .is_empty());
}

#[test]
fn a_fallback_focus_does_not_mark_the_event_read() -> Result<(), Box<dyn std::error::Error>> {
    let shared = workspace(&event_with_candidates("shared", 1, &["0xguess", "0xother"]))?;

    assert_eq!(shared.focus_outcome(Some("0xguess")), FocusOutcome::Primary);
    assert_eq!(
        shared.focus_outcome(Some("0xother")),
        FocusOutcome::Fallback
    );
    assert_eq!(shared.focus_outcome(None), FocusOutcome::NotFocused);
    Ok(())
}

#[test]
fn the_source_is_certain_only_when_the_focused_window_is_the_sole_candidate(
) -> Result<(), Box<dyn std::error::Error>> {
    let sole = workspace(&event_with_candidates("sole", 1, &["0xonly"]))?;
    let shared = workspace(&event_with_candidates("shared", 1, &["0xguess", "0xother"]))?;
    let legacy = workspace(&event_with_address("legacy", 1, "0xlegacy"))?;

    assert!(sole.is_sole_candidate("0xonly"));
    assert!(!shared.is_sole_candidate("0xguess"));
    assert!(!shared.is_sole_candidate("0xother"));
    assert!(legacy.is_sole_candidate("0xlegacy"));
    assert!(!legacy.is_sole_candidate("0xelse"));
    Ok(())
}

#[test]
fn candidate_addresses_serialize_additively() -> Result<(), Box<dyn std::error::Error>> {
    let legacy_json = serde_json::to_string(&event_with_address("legacy", 1, "0xbeef"))?;
    assert!(!legacy_json.contains("clientAddresses"));

    let event = event_with_candidates("guessed", 4682, &["0xguess", "0xother"]);
    let value = serde_json::to_value(&event)?;
    assert_eq!(value["workspace"]["clientAddresses"][0], "0xguess");
    assert_eq!(value["workspace"]["clientAddresses"][1], "0xother");
    Ok(())
}

#[test]
fn a_parsed_v1_workspace_without_the_candidate_list_falls_back_to_the_primary(
) -> Result<(), Box<dyn std::error::Error>> {
    let raw = r#"{"version":1,"events":[{"id":"e","agent":"claude","kind":"main",
        "projectName":"p","projectPath":"/repo/dotfiles","cwd":"/repo/dotfiles",
        "sessionId":"s","createdAt":"2026-07-26T08:00:00.000Z",
        "workspace":{"id":1,"name":"1","monitor":"DP-3","clientPid":42,
            "clientAddress":"0xbeef","title":"t"},
        "status":"unread"}]}"#;
    let state = parse_state(raw)?;
    let workspace = state
        .events
        .first()
        .and_then(|event| event.workspace.clone())
        .ok_or("missing workspace")?;

    assert_eq!(workspace.candidate_addresses(), ["0xbeef"]);
    Ok(())
}

#[test]
fn prunes_events_without_live_hyprland_addresses() {
    let state = state_of(vec![
        event_with_address("live-window", 42, "0xlive"),
        event_with_address("stale-window", 7, "0xstale"),
        AgentEvent {
            workspace: None,
            ..event_with_session("session-only", "session-only")
        },
    ]);
    let existing_addresses = HashSet::from(["0xlive".to_owned()]);

    let pruned = prune_stale_events(state, &existing_addresses);

    assert_eq!(pruned.events.len(), 1);
    assert_eq!(
        pruned.events.first().map(|event| event.id.as_str()),
        Some("live-window")
    );
}
