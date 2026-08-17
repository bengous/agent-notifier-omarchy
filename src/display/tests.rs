use super::*;
use crate::event::SourceWindow;
use crate::test_fixtures::{
    base_event, event_in_project, event_with_address, event_with_pid, nothing_installed_probe,
    state_of, wired_probe, workspace,
};

fn displayed_projects(events: Vec<AgentEvent>) -> Vec<(String, String)> {
    display_state_from_events(1, events, &wired_probe())
        .events
        .into_iter()
        .map(|row| (row.event.id, row.display_project))
        .collect()
}

fn build_info_fixture(state_path: Option<&Path>) -> BuildInfo {
    build_info(
        "agent-notifier",
        "0.3.0",
        "abc1234",
        "true",
        "2026-08-16T10:00:00+00:00",
        state_path,
    )
}

#[test]
fn strips_any_leading_spinner_glyph() {
    // U+28F8 is not in the historical blocklist.
    assert_eq!(clean_window_title("⣸ building"), "building");
    assert_eq!(clean_window_title("\u{28f8} building"), "building");
    assert_eq!(clean_window_title("◑ building"), "building");
    assert_eq!(clean_window_title("◜ building"), "building");
    assert_eq!(clean_window_title("◷ building"), "building");
    assert_eq!(clean_window_title("✻ building"), "building");
    assert_eq!(clean_window_title("  plain title  "), "plain title");
    assert_eq!(clean_window_title("~/dotfiles"), "~/dotfiles");
}

#[test]
fn display_state_exposes_exactly_the_keys_the_widget_reads() {
    let state = display_state_from_events(
        1,
        vec![event_with_address("e", 300, "0xbeef")],
        &wired_probe(),
    );
    let value = serde_json::to_value(&state).unwrap_or(serde_json::Value::Null);
    let event = &value["events"][0];

    for key in [
        "id",
        "agent",
        "status",
        "createdAt",
        "displayLabel",
        "displayProject",
    ] {
        assert!(!event[key].is_null(), "missing key: {key}");
    }
    assert!(value["events"].is_array());
}

#[test]
fn the_setup_summary_exposes_exactly_the_keys_the_widget_reads(
) -> Result<(), Box<dyn std::error::Error>> {
    let value = serde_json::to_value(setup_summary(&wired_probe()))?;
    let summary = value.as_object().ok_or("summary is not an object")?;
    let mut summary_keys = summary.keys().map(String::as_str).collect::<Vec<_>>();
    summary_keys.sort_unstable();
    assert_eq!(summary_keys, ["harnesses", "listenerLive", "ready"]);

    let rows = value["harnesses"]
        .as_array()
        .ok_or("harnesses is not an array")?;
    let names = rows
        .iter()
        .map(|row| row["harness"].clone())
        .collect::<Vec<_>>();
    assert_eq!(names, ["claude", "codex", "pi"]);
    for row in rows {
        let row = row.as_object().ok_or("row is not an object")?;
        let mut row_keys = row.keys().map(String::as_str).collect::<Vec<_>>();
        row_keys.sort_unstable();
        assert_eq!(row_keys, ["displayName", "harness", "state"]);
    }
    Ok(())
}

#[test]
fn the_status_setup_field_serializes_additively() -> Result<(), Box<dyn std::error::Error>> {
    let with_setup = status_output(&[], &wired_probe());
    let bare = StatusOutput {
        setup: None,
        ..with_setup.clone()
    };
    let with_value = serde_json::to_value(&with_setup)?;
    let mut bare_value = serde_json::to_value(&bare)?;

    assert!(bare_value.get("setup").is_none());
    bare_value["setup"] = with_value["setup"].clone();
    assert_eq!(with_value, bare_value);
    Ok(())
}

#[test]
fn the_display_state_setup_field_serializes_additively() -> Result<(), Box<dyn std::error::Error>> {
    let with_setup = display_state_from_events(1, Vec::new(), &wired_probe());
    let bare = DisplayState {
        setup: None,
        ..with_setup.clone()
    };
    let with_value = serde_json::to_value(&with_setup)?;
    let mut bare_value = serde_json::to_value(&bare)?;

    assert!(bare_value.get("setup").is_none());
    bare_value["setup"] = with_value["setup"].clone();
    assert_eq!(with_value, bare_value);
    Ok(())
}

#[test]
fn version_json_exposes_exactly_the_keys_the_widget_reads() {
    let value = serde_json::to_value(build_info_fixture(None)).unwrap_or(serde_json::Value::Null);

    for key in ["version", "commit", "dirty", "commitDate"] {
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
    let dirty = |flag: &str| build_info("n", "v", "c", flag, "d", None).dirty;

    assert!(dirty("true"));
    assert!(!dirty("false"));
    assert!(!dirty(""));
    assert!(!dirty("garbage"));

    let fallback = build_info("n", "v", "unknown", "false", "unknown", None);
    assert_eq!(fallback.commit, "unknown");
    assert_eq!(fallback.commit_date, "unknown");
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
fn the_event_label_is_the_cleaned_window_title() -> Result<(), Box<dyn std::error::Error>> {
    let titled = AgentEvent {
        workspace: Some(SourceWindow {
            title: "⠴ dotfiles | feature".to_owned(),
            ..workspace(&base_event())?
        }),
        ..base_event()
    };

    assert_eq!(event_label(&titled), "dotfiles | feature");
    Ok(())
}

#[test]
fn an_empty_window_title_falls_back_to_the_project_and_its_branch(
) -> Result<(), Box<dyn std::error::Error>> {
    let untitled = AgentEvent {
        workspace: Some(SourceWindow {
            title: String::new(),
            ..workspace(&base_event())?
        }),
        ..base_event()
    };

    assert_eq!(event_label(&untitled), "dotfiles | main");
    Ok(())
}

#[test]
fn an_event_without_a_source_window_falls_back_to_the_project_and_its_branch() {
    let windowless = AgentEvent {
        workspace: None,
        ..base_event()
    };

    assert_eq!(event_label(&windowless), "dotfiles | main");
}

#[test]
fn an_event_without_a_branch_falls_back_to_the_project_name() {
    let branchless = AgentEvent {
        branch_name: None,
        workspace: None,
        ..base_event()
    };

    assert_eq!(event_label(&branchless), "dotfiles");
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
fn formats_agent_button_label() {
    assert_eq!(format_agent_button(0), "agents");
    assert_eq!(format_agent_button(3), "agents 󰂚 3");
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
    let output = status_output(&dedupe_events(state.events), &wired_probe());
    assert_eq!(output.text, "agents 󰂚 2");
    assert_eq!(output.class, "unread");
}

#[test]
fn the_status_tooltip_is_the_window_title_and_the_completion_time(
) -> Result<(), Box<dyn std::error::Error>> {
    let state = state_of(vec![AgentEvent {
        workspace: Some(SourceWindow {
            title: "line\nbreak".to_owned(),
            ..workspace(&base_event())?
        }),
        ..base_event()
    }]);
    let output = status_output(&dedupe_events(state.events), &wired_probe());
    assert_eq!(output.text, "agents 󰂚 1");
    assert_eq!(output.tooltip, "line\nbreak May 6, 2026 10:00 AM UTC");
    assert_eq!(output.class, "unread");
    Ok(())
}

#[test]
fn a_read_only_status_waits_for_completions_when_the_setup_is_ready() {
    let mut event = base_event();
    event.status = EventStatus::Read;
    let output = status_output(&dedupe_events(vec![event]), &wired_probe());
    assert_eq!(output.text, "agents");
    assert_eq!(output.tooltip, "Waiting for agent completions");
    assert_eq!(output.class, "empty");
}

#[test]
fn an_empty_status_with_an_unready_setup_asks_for_setup() {
    let output = status_output(&[], &nothing_installed_probe());
    assert_eq!(output.text, "agents");
    assert_eq!(
        output.tooltip,
        "Setup required — run: agent-notifier doctor"
    );
    assert_eq!(output.class, "setup");
}

#[test]
fn unread_events_keep_the_unread_class_over_setup() {
    let output = status_output(&[event_with_pid("event-1", 1)], &nothing_installed_probe());
    assert_eq!(output.class, "unread");
    assert!(output.tooltip.contains("dotfiles | main"));
}

#[test]
fn the_unavailable_status_is_visibly_an_error() {
    let output = unavailable_status_output();
    assert_eq!(output.text, "agents !");
    assert_eq!(output.class, "error");
}

#[test]
fn the_unavailable_status_literal_serializes_the_unavailable_output(
) -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        serde_json::to_string(&unavailable_status_output())?,
        UNAVAILABLE_STATUS_JSON
    );
    Ok(())
}
