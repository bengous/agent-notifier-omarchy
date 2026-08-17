use super::*;
use crate::event::parse_state;
use crate::hook_failure_exit_code;
use crate::test_fixtures::{event_with_address, fixture_clock};
use std::fs;
use tempfile::tempdir;

#[test]
fn the_build_script_always_supplies_commit_metadata() {
    assert!(!env!("AGENT_NOTIFIER_COMMIT").is_empty());
    assert!(!env!("AGENT_NOTIFIER_DIRTY").is_empty());
    assert!(!env!("AGENT_NOTIFIER_COMMIT_DATE").is_empty());
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

struct FailingJson;

impl Serialize for FailingJson {
    fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("unserializable"))
    }
}

#[test]
fn a_serialization_failure_propagates_instead_of_printing_a_status_shape() {
    assert!(print_json(&FailingJson).is_err());
}

#[test]
fn a_read_query_marks_the_focused_windows_events_read() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("events.json");
    let now = fixture_clock()?;
    with_state_update(&path, now, |state| {
        let state = append_and_trim(state, event_with_address("focused", 1, "0xfocused"));
        append_and_trim(state, event_with_address("background", 2, "0xother"))
    })?;

    let state = read_state_with_focused_window_read(&path, Some("0xfocused"), now)?;

    let status_of = |events: &[AgentEvent], id: &str| {
        events
            .iter()
            .find(|event| event.id == id)
            .map(|event| event.status)
    };
    assert_eq!(status_of(&state.events, "focused"), Some(EventStatus::Read));
    assert_eq!(
        status_of(&state.events, "background"),
        Some(EventStatus::Unread)
    );
    let stored = parse_state(&fs::read_to_string(&path)?)?;
    assert_eq!(
        status_of(&stored.events, "focused"),
        Some(EventStatus::Read)
    );
    Ok(())
}

#[test]
fn a_read_query_that_changes_nothing_skips_the_state_rewrite(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("events.json");
    let now = fixture_clock()?;
    with_state_update(&path, now, |state| {
        append_and_trim(state, event_with_address("unread", 1, "0xaway"))
    })?;
    let modified = fs::metadata(&path)?.modified()?;
    thread::sleep(Duration::from_millis(20));

    let _ = read_state_with_focused_window_read(&path, Some("0xelsewhere"), now)?;

    assert_eq!(fs::metadata(&path)?.modified()?, modified);
    Ok(())
}
