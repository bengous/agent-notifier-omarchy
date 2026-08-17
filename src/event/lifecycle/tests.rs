use super::*;
use crate::display::status_output;
use crate::event::store::with_state_update;
use crate::event::{empty_state, parse_state};
use crate::test_fixtures::{
    base_event, base_pi_event, event_with_address, event_with_candidates, event_with_pid,
    event_with_session, event_with_source_process, fixture_clock, sessionless_event, state_of,
    workspace,
};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn liveness_of(addresses: &[&str], process_is_alive: fn(&ProcessRef) -> bool) -> SourceLiveness {
    SourceLiveness {
        existing_addresses: addresses
            .iter()
            .map(|address| (*address).to_owned())
            .collect(),
        process_is_alive,
    }
}

fn stored_event_ids(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(parse_state(&fs::read_to_string(path)?)?
        .events
        .into_iter()
        .map(|event| event.id)
        .collect())
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
fn a_read_with_a_focused_window_marks_only_its_events_read() {
    let state = state_of(vec![
        event_with_address("focused", 1, "0xfocused"),
        event_with_address("other", 2, "0xother"),
    ]);
    let state = mark_focused_window_events_read(state, Some("0xfocused"));

    assert_eq!(
        status_output(&dedupe_events(state.events)).text,
        "agents 󰂚 1"
    );
}

#[test]
fn a_read_without_a_focused_window_changes_nothing() {
    let state = state_of(vec![event_with_address("unread", 1, "0xaway")]);

    assert_eq!(mark_focused_window_events_read(state.clone(), None), state);
}

#[test]
fn captures_only_events_with_source_addresses() {
    let no_window = AgentEvent {
        workspace: None,
        ..base_event()
    };
    assert_eq!(
        capture_decision(&no_window, None),
        CaptureDecision::AlertOnly
    );
    assert_eq!(
        capture_decision(&base_pi_event(None), None),
        CaptureDecision::AlertOnly
    );
    assert_eq!(
        capture_decision(&base_event(), None),
        CaptureDecision::AlertOnly
    );
    assert_eq!(
        capture_decision(&event_with_address("addressed", 300, "0xbeef"), None),
        CaptureDecision::PersistAndAlert
    );
}

#[test]
fn a_completion_from_the_focused_sole_candidate_window_is_discarded() {
    let sole = event_with_address("sole", 1, "0xonly");

    assert_eq!(
        capture_decision(&sole, Some("0xonly")),
        CaptureDecision::Discard
    );
    assert_eq!(
        capture_decision(&sole, Some("0xelse")),
        CaptureDecision::PersistAndAlert
    );
}

#[test]
fn an_uncertain_source_is_captured_even_when_its_best_guess_holds_the_focus() {
    let shared = event_with_candidates("shared", 1, &["0xguess", "0xother"]);

    assert_eq!(
        capture_decision(&shared, Some("0xguess")),
        CaptureDecision::PersistAndAlert
    );
}

#[test]
fn a_recycled_pid_does_not_resurrect_a_dead_event() {
    let dead = event_with_address("dead", 300, "0xdead");
    // The window that owned pid 300 is gone; an unrelated live window now has it.
    let liveness = liveness_of(&["0xbeef"], |_| false);

    assert!(focusable_events(&[dead], &liveness).is_empty());
}

#[test]
fn an_event_is_focusable_while_its_address_still_exists() {
    let alive = event_with_address("alive", 300, "0xbeef");
    let liveness = liveness_of(&["0xbeef"], |_| false);

    assert_eq!(focusable_events(&[alive], &liveness).len(), 1);
}

#[test]
fn legacy_events_without_an_address_are_not_focusable() {
    let legacy = event_with_pid("legacy", 300);
    let liveness = liveness_of(&["0xbeef"], |_| false);

    assert!(focusable_events(&[legacy], &liveness).is_empty());
}

#[test]
fn excludes_stored_events_when_no_source_window_exists() {
    let events = vec![
        AgentEvent {
            workspace: None,
            ..event_with_session("stale-session-only", "session-stale")
        },
        event_with_address("stale-window", 42, "0xstale"),
    ];

    let focusable = focusable_events(&events, &liveness_of(&[], |_| false));

    assert!(focusable.is_empty());
    assert_eq!(status_output(&focusable).text, "agents");
}

#[test]
fn includes_only_events_matching_existing_hyprland_addresses() {
    let events = vec![
        event_with_address("live-window", 42, "0xlive"),
        event_with_address("stale-window", 7, "0xstale"),
        AgentEvent {
            workspace: None,
            ..event_with_session("session-only", "session-only")
        },
    ];

    let focusable = focusable_events(&events, &liveness_of(&["0xlive"], |_| false));

    assert_eq!(focusable.len(), 1);
    assert_eq!(
        focusable.first().map(|event| event.id.as_str()),
        Some("live-window")
    );
    assert_eq!(status_output(&focusable).text, "agents 󰂚 1");
}

#[test]
fn an_event_with_a_live_source_process_stays_focusable_without_existing_windows() {
    let event = event_with_source_process(
        "session-alive",
        ProcessRef {
            pid: 146_082,
            start_time: 737_679,
        },
    );

    let focusable = focusable_events(&[event], &liveness_of(&[], |_| true));

    assert_eq!(focusable.len(), 1);
}

#[test]
fn an_event_with_a_dead_source_process_is_pruned_despite_existing_candidates() {
    let event = event_with_source_process(
        "session-dead",
        ProcessRef {
            pid: 146_082,
            start_time: 737_679,
        },
    );
    let state = state_of(vec![event]);

    assert!(
        prune_stale_events(state, &liveness_of(&["0xguess"], |_| false))
            .events
            .is_empty()
    );
}

#[test]
fn an_event_with_any_existing_candidate_stays_focusable() {
    let event = event_with_candidates("guessed", 4682, &["0xguess", "0xother"]);

    assert_eq!(
        focusable_events(&[event], &liveness_of(&["0xother"], |_| false)).len(),
        1
    );
}

#[test]
fn an_event_with_no_existing_candidate_is_pruned() {
    let state = state_of(vec![event_with_candidates(
        "guessed",
        4682,
        &["0xguess", "0xother"],
    )]);

    assert!(
        prune_stale_events(state, &liveness_of(&["0xelse"], |_| false))
            .events
            .is_empty()
    );
}

#[test]
fn prunes_events_without_existing_hyprland_windows() {
    let state = state_of(vec![
        event_with_address("live-window", 42, "0xlive"),
        event_with_address("stale-window", 7, "0xstale"),
        AgentEvent {
            workspace: None,
            ..event_with_session("session-only", "session-only")
        },
    ]);

    let pruned = prune_stale_events(state, &liveness_of(&["0xlive"], |_| false));

    assert_eq!(pruned.events.len(), 1);
    assert_eq!(
        pruned.events.first().map(|event| event.id.as_str()),
        Some("live-window")
    );
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
    let liveness = liveness_of(&["0xlive"], |_| false);

    with_state_update(&path, now, |state| prune_stale_events(state, &liveness))?;

    assert_eq!(stored_event_ids(&path)?, ["here"]);
    Ok(())
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
