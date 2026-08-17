use super::*;
use crate::event::store::with_state_update;
use crate::event::{append_and_trim, parse_state, AgentEvent, EventStatus};
use crate::test_fixtures::{event_with_address, FakeDeps};
use std::error::Error;
use std::fs;
use std::thread;
use std::time::Duration;
use tempfile::{tempdir, TempDir};

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
    let deps = fake(&dir);

    assert!(print_json(&FailingJson, &deps).is_err());
    Ok(())
}

#[test]
fn a_read_query_marks_the_focused_windows_events_read() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    let deps = fake(&dir);
    seed(
        &deps,
        vec![
            event_with_address("focused", 1, "0xfocused"),
            event_with_address("background", 2, "0xother"),
        ],
    )?;

    let state = read_state_marking_focused_window_read(&deps, Some("0xfocused"))?;

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
    let dir = tempdir()?;
    let deps = fake(&dir);
    seed(&deps, vec![event_with_address("unread", 1, "0xaway")])?;
    let modified = fs::metadata(&deps.state_path)?.modified()?;
    thread::sleep(Duration::from_millis(20));

    let _ = read_state_marking_focused_window_read(&deps, Some("0xelsewhere"))?;

    assert_eq!(fs::metadata(&deps.state_path)?.modified()?, modified);
    Ok(())
}

fn fake(dir: &TempDir) -> FakeDeps {
    FakeDeps::new(dir.path().join("events.json"))
}

fn seed(deps: &FakeDeps, events: Vec<AgentEvent>) -> Result<(), Box<dyn Error>> {
    with_state_update(&deps.state_path, deps.now, |state| {
        events.into_iter().fold(state, append_and_trim)
    })?;
    Ok(())
}

fn status_of(events: &[AgentEvent], id: &str) -> Option<EventStatus> {
    events
        .iter()
        .find(|event| event.id == id)
        .map(|event| event.status)
}
