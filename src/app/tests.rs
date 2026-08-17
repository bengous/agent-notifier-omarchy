use super::*;
use crate::event::{parse_state, SourceWindow};
use crate::hook_failure_exit_code;
use crate::test_fixtures::{event_with_address, workspace, FakeDeps};
use serde_json::Value;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::thread;
use std::time::Duration;
use tempfile::{tempdir, TempDir};

#[test]
fn the_build_script_always_supplies_commit_metadata() {
    assert!(!env!("AGENT_NOTIFIER_COMMIT").is_empty());
    assert!(!env!("AGENT_NOTIFIER_DIRTY").is_empty());
    assert!(!env!("AGENT_NOTIFIER_COMMIT_DATE").is_empty());
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

struct FailingJson;

impl Serialize for FailingJson {
    fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("unserializable"))
    }
}

#[test]
fn a_serialization_failure_propagates_instead_of_printing_a_status_shape(
) -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    let deps = fake(&dir)?;
    assert!(print_json(&FailingJson, &deps).is_err());
    Ok(())
}

#[test]
fn a_read_query_marks_the_focused_windows_events_read() -> Result<(), Box<dyn Error>> {
    let deps = FakeDeps {
        focused_window_address: Some("0xfocused".to_owned()),
        ..fake(&tempdir()?)?
    };
    seed(
        &deps,
        vec![
            event_with_address("focused", 1, "0xfocused"),
            event_with_address("background", 2, "0xother"),
        ],
    )?;

    let state = read_state_with_focused_window_read(&deps, Some("0xfocused"))?;

    assert_eq!(status_of(&state.events, "focused"), Some(EventStatus::Read));
    assert_eq!(
        status_of(&state.events, "background"),
        Some(EventStatus::Unread)
    );
    let stored = parse_state(&fs::read_to_string(&deps.state_path)?)?;
    assert_eq!(
        status_of(&stored.events, "focused"),
        Some(EventStatus::Read)
    );
    Ok(())
}

#[test]
fn a_read_query_that_changes_nothing_skips_the_state_rewrite() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;
    seed(&deps, vec![event_with_address("unread", 1, "0xaway")])?;
    let modified = fs::metadata(&deps.state_path)?.modified()?;
    thread::sleep(Duration::from_millis(20));

    let _ = read_state_with_focused_window_read(&deps, Some("0xelsewhere"))?;

    assert_eq!(fs::metadata(&deps.state_path)?.modified()?, modified);
    Ok(())
}

#[test]
fn help_prints_the_usage() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;

    assert_eq!(run(&CliCommand::Help, &deps)?, 0);

    assert!(deps.printed().contains("Usage: agent-notifier <command>"));
    Ok(())
}

#[test]
fn version_prints_the_crate_version() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;

    assert_eq!(run(&CliCommand::Version, &deps)?, 0);

    assert_eq!(
        deps.printed(),
        format!("agent-notifier {}", env!("CARGO_PKG_VERSION"))
    );
    Ok(())
}

#[test]
fn the_codex_hook_stores_the_completion_and_alerts() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    let deps = FakeDeps {
        stdin: hook_payload(&dir, None),
        source_window: Some(source_window("0xsource")?),
        ..fake(&dir)?
    };

    assert_eq!(run(&CliCommand::Hook, &deps)?, 0);

    assert_eq!(deps.stored_state()?.version, 1);
    let event = only_stored_event(&deps)?;
    assert_eq!(event.agent, "codex");
    assert_eq!(event.status, EventStatus::Unread);
    assert!(event.id.starts_with("1778061600000-"));
    assert_eq!(
        event
            .workspace
            .as_ref()
            .and_then(|window| window.client_address.as_deref()),
        Some("0xsource")
    );
    assert_eq!(
        deps.alerts.borrow().as_slice(),
        [[
            "Codex".to_owned(),
            "Codex completed".to_owned(),
            "dotfiles | main".to_owned()
        ]]
    );
    Ok(())
}

#[test]
fn the_pi_hook_stores_a_pi_completion() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    let deps = FakeDeps {
        stdin: hook_payload(&dir, Some("pi-session")),
        source_window: Some(source_window("0xpi")?),
        ..fake(&dir)?
    };

    assert_eq!(run(&CliCommand::PiHook, &deps)?, 0);

    let event = only_stored_event(&deps)?;
    assert_eq!(event.agent, "pi");
    assert_eq!(event.session_id, "pi-session");
    assert_eq!(deps.alerts.borrow().len(), 1);
    Ok(())
}

#[test]
fn the_claude_hook_stores_a_claude_completion() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    let deps = FakeDeps {
        stdin: hook_payload(&dir, Some("claude-session")),
        source_window: Some(source_window("0xclaude")?),
        ..fake(&dir)?
    };

    assert_eq!(run(&CliCommand::ClaudeHook, &deps)?, 0);

    let event = only_stored_event(&deps)?;
    assert_eq!(event.agent, "claude");
    assert_eq!(event.session_id, "claude-session");
    assert_eq!(deps.alerts.borrow().len(), 1);
    Ok(())
}

#[test]
fn a_completion_on_the_focused_window_is_neither_stored_nor_alerted() -> Result<(), Box<dyn Error>>
{
    let dir = tempdir()?;
    let deps = FakeDeps {
        stdin: hook_payload(&dir, None),
        source_window: Some(source_window("0xsource")?),
        focused_window_address: Some("0xsource".to_owned()),
        ..fake(&dir)?
    };

    assert_eq!(run(&CliCommand::Hook, &deps)?, 0);

    assert!(!deps.state_path.exists());
    assert!(deps.alerts.borrow().is_empty());
    Ok(())
}

#[test]
fn status_json_prints_the_widget_shape_and_marks_the_focused_window_read(
) -> Result<(), Box<dyn Error>> {
    let deps = FakeDeps {
        focused_window_address: Some("0xfocused".to_owned()),
        existing_window_addresses: addresses(&["0xfocused", "0xother"]),
        ..fake(&tempdir()?)?
    };
    seed(
        &deps,
        vec![
            event_with_address("focused", 1, "0xfocused"),
            event_with_address("background", 2, "0xother"),
        ],
    )?;

    assert_eq!(run(&CliCommand::StatusJson, &deps)?, 0);

    let status = deps.printed_json()?;
    assert_eq!(sorted_keys(&status)?, ["class", "text", "tooltip"]);
    assert_eq!(status["text"], "agents 󰂚 1");
    assert_eq!(status["class"], "unread");
    assert_eq!(deps.stored_event("focused")?.status, EventStatus::Read);
    assert_eq!(deps.stored_event("background")?.status, EventStatus::Unread);
    Ok(())
}

#[test]
fn list_display_json_prints_the_display_fields_and_marks_the_focused_window_read(
) -> Result<(), Box<dyn Error>> {
    let deps = FakeDeps {
        focused_window_address: Some("0xfocused".to_owned()),
        existing_window_addresses: addresses(&["0xfocused"]),
        ..fake(&tempdir()?)?
    };
    seed(&deps, vec![event_with_address("focused", 1, "0xfocused")])?;

    assert_eq!(run(&CliCommand::ListDisplayJson, &deps)?, 0);

    let display = deps.printed_json()?;
    assert_eq!(sorted_keys(&display)?, ["events", "version"]);
    assert_eq!(display["version"], 1);
    let event = &display["events"][0];
    assert_eq!(event["id"], "focused");
    assert_eq!(event["displayLabel"], "dotfiles | main");
    assert_eq!(event["displayProject"], "dotfiles");
    assert!(event["displayCreatedAt"].is_string());
    assert_eq!(deps.stored_event("focused")?.status, EventStatus::Read);
    Ok(())
}

#[test]
fn list_json_prints_the_stored_state() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;
    seed(&deps, vec![event_with_address("stored", 1, "0xstored")])?;

    assert_eq!(run(&CliCommand::ListJson, &deps)?, 0);

    let state = deps.printed_json()?;
    assert_eq!(sorted_keys(&state)?, ["events", "version"]);
    assert_eq!(state["version"], 1);
    assert_eq!(state["events"][0]["id"], "stored");
    assert_eq!(state["events"][0]["status"], "unread");
    Ok(())
}

#[test]
fn version_json_prints_the_build_metadata_and_the_state_path() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;

    assert_eq!(run(&CliCommand::VersionJson, &deps)?, 0);

    let info = deps.printed_json()?;
    assert_eq!(
        sorted_keys(&info)?,
        [
            "commit",
            "commitDate",
            "dirty",
            "name",
            "statePath",
            "version"
        ]
    );
    assert_eq!(info["name"], "agent-notifier");
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        info["statePath"],
        deps.state_path.to_string_lossy().as_ref()
    );
    Ok(())
}

#[test]
fn focus_id_focuses_the_source_window_and_marks_the_event_read() -> Result<(), Box<dyn Error>> {
    let deps = FakeDeps {
        focus_outcome: FocusOutcome::Primary,
        ..fake(&tempdir()?)?
    };
    seed(&deps, vec![event_with_address("target", 1, "0xtarget")])?;

    assert_eq!(run(&CliCommand::FocusId("target".to_owned()), &deps)?, 0);

    assert_eq!(deps.stored_event("target")?.status, EventStatus::Read);
    Ok(())
}

#[test]
fn focus_id_reports_an_event_it_cannot_focus() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;
    seed(&deps, vec![event_with_address("target", 1, "0xtarget")])?;

    assert_eq!(run(&CliCommand::FocusId("unknown".to_owned()), &deps)?, 1);

    assert_eq!(deps.stored_event("target")?.status, EventStatus::Unread);
    Ok(())
}

#[test]
fn a_fallback_focus_leaves_the_event_unread() -> Result<(), Box<dyn Error>> {
    let deps = FakeDeps {
        focus_outcome: FocusOutcome::Fallback,
        ..fake(&tempdir()?)?
    };
    seed(&deps, vec![event_with_address("target", 1, "0xtarget")])?;

    assert_eq!(run(&CliCommand::FocusId("target".to_owned()), &deps)?, 0);

    assert_eq!(deps.stored_event("target")?.status, EventStatus::Unread);
    Ok(())
}

#[test]
fn focus_id_without_an_id_is_a_usage_error() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;

    assert_eq!(run(&CliCommand::FocusIdMissing, &deps)?, 2);

    Ok(())
}

#[test]
fn focus_latest_focuses_the_newest_unread_event() -> Result<(), Box<dyn Error>> {
    let deps = FakeDeps {
        focus_outcome: FocusOutcome::Primary,
        existing_window_addresses: addresses(&["0xold", "0xnew"]),
        ..fake(&tempdir()?)?
    };
    seed(
        &deps,
        vec![
            event_with_address("old", 1, "0xold"),
            event_with_address("new", 2, "0xnew"),
        ],
    )?;

    assert_eq!(run(&CliCommand::FocusLatest, &deps)?, 0);

    assert_eq!(deps.stored_event("new")?.status, EventStatus::Read);
    assert_eq!(deps.stored_event("old")?.status, EventStatus::Unread);
    Ok(())
}

#[test]
fn mark_read_marks_the_event_read() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;
    seed(&deps, vec![event_with_address("target", 1, "0xtarget")])?;

    assert_eq!(run(&CliCommand::MarkRead("target".to_owned()), &deps)?, 0);

    assert_eq!(deps.stored_event("target")?.status, EventStatus::Read);
    Ok(())
}

#[test]
fn mark_read_without_an_id_is_a_usage_error() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;

    assert_eq!(run(&CliCommand::MarkReadMissing, &deps)?, 2);

    Ok(())
}

#[test]
fn focused_window_read_marks_the_focused_windows_events_read() -> Result<(), Box<dyn Error>> {
    let deps = FakeDeps {
        focused_window_address: Some("0xfocused".to_owned()),
        ..fake(&tempdir()?)?
    };
    seed(
        &deps,
        vec![
            event_with_address("focused", 1, "0xfocused"),
            event_with_address("background", 2, "0xother"),
        ],
    )?;

    assert_eq!(run(&CliCommand::FocusedWindowRead, &deps)?, 0);

    assert_eq!(deps.stored_event("focused")?.status, EventStatus::Read);
    assert_eq!(deps.stored_event("background")?.status, EventStatus::Unread);
    Ok(())
}

#[test]
fn watch_focused_window_marks_every_reported_window_read() -> Result<(), Box<dyn Error>> {
    let deps = FakeDeps {
        focused_window_changes: vec!["0xfirst".to_owned(), "0xsecond".to_owned()],
        ..fake(&tempdir()?)?
    };
    seed(
        &deps,
        vec![
            event_with_address("first", 1, "0xfirst"),
            event_with_address("second", 2, "0xsecond"),
            event_with_address("untouched", 3, "0xother"),
        ],
    )?;

    assert_eq!(run(&CliCommand::WatchFocusedWindow, &deps)?, 0);

    assert_eq!(deps.stored_event("first")?.status, EventStatus::Read);
    assert_eq!(deps.stored_event("second")?.status, EventStatus::Read);
    assert_eq!(deps.stored_event("untouched")?.status, EventStatus::Unread);
    Ok(())
}

#[test]
fn clear_read_keeps_only_the_unread_events() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;
    seed(
        &deps,
        vec![
            event_with_address("unread", 1, "0xunread"),
            AgentEvent {
                status: EventStatus::Read,
                ..event_with_address("read", 2, "0xread")
            },
        ],
    )?;

    assert_eq!(run(&CliCommand::ClearRead, &deps)?, 0);

    let ids = deps
        .stored_state()?
        .events
        .into_iter()
        .map(|event| event.id)
        .collect::<Vec<_>>();
    assert_eq!(ids, ["unread"]);
    Ok(())
}

#[test]
fn clear_all_empties_the_state() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;
    seed(&deps, vec![event_with_address("stored", 1, "0xstored")])?;

    assert_eq!(run(&CliCommand::ClearAll, &deps)?, 0);

    let state = deps.stored_state()?;
    assert_eq!(state.version, 1);
    assert!(state.events.is_empty());
    Ok(())
}

#[test]
fn prune_stale_drops_the_events_whose_window_is_gone() -> Result<(), Box<dyn Error>> {
    let deps = FakeDeps {
        existing_window_addresses: addresses(&["0xalive"]),
        ..fake(&tempdir()?)?
    };
    seed(
        &deps,
        vec![
            event_with_address("alive", 1, "0xalive"),
            event_with_address("gone", 2, "0xgone"),
        ],
    )?;

    assert_eq!(run(&CliCommand::PruneStale, &deps)?, 0);

    let ids = deps
        .stored_state()?
        .events
        .into_iter()
        .map(|event| event.id)
        .collect::<Vec<_>>();
    assert_eq!(ids, ["alive"]);
    Ok(())
}

#[test]
fn an_unknown_command_is_a_usage_error() -> Result<(), Box<dyn Error>> {
    let deps = fake(&tempdir()?)?;

    assert_eq!(run(&CliCommand::Unknown, &deps)?, 2);

    Ok(())
}

fn fake(dir: &TempDir) -> Result<FakeDeps, Box<dyn Error>> {
    FakeDeps::new(dir.path().join("agent-notifier/events.json"))
}

fn hook_payload(dir: &TempDir, session_id: Option<&str>) -> String {
    let session = session_id.map_or_else(String::new, |id| format!(r#","session_id":"{id}""#));
    format!(r#"{{"cwd":"{}"{session}}}"#, dir.path().display())
}

fn source_window(address: &str) -> Result<SourceWindow, Box<dyn Error>> {
    workspace(&event_with_address("source", 300, address))
}

fn addresses(addresses: &[&str]) -> HashSet<String> {
    addresses
        .iter()
        .map(|address| (*address).to_owned())
        .collect()
}

fn seed(deps: &FakeDeps, events: Vec<AgentEvent>) -> Result<(), Box<dyn Error>> {
    with_state_update(&deps.state_path, deps.now, |state| {
        events.into_iter().fold(state, append_and_trim)
    })?;
    Ok(())
}

fn only_stored_event(deps: &FakeDeps) -> Result<AgentEvent, Box<dyn Error>> {
    match <[AgentEvent; 1]>::try_from(deps.stored_state()?.events) {
        Ok([event]) => Ok(event),
        Err(events) => {
            Err(format!("expected exactly one stored event, got {}", events.len()).into())
        }
    }
}

fn status_of(events: &[AgentEvent], id: &str) -> Option<EventStatus> {
    events
        .iter()
        .find(|event| event.id == id)
        .map(|event| event.status)
}

fn sorted_keys(value: &Value) -> Result<Vec<String>, Box<dyn Error>> {
    let mut keys = value
        .as_object()
        .ok_or("expected a JSON object")?
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    Ok(keys)
}
