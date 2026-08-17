use super::*;
use crate::display::status_output;
use crate::event::empty_state;
use crate::test_fixtures::{
    base_event, event_with_address, event_with_candidates, event_with_session, sessionless_event,
    state_of, workspace,
};

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
