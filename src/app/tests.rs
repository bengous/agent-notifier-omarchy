use super::*;
use crate::event::{parse_state, ProcessRef};
use crate::hook_failure_exit_code;
use crate::test_fixtures::{
    base_event, base_pi_event, event_with_address, event_with_candidates, event_with_pid,
    event_with_session, event_with_source_process, fixture_clock, state_of,
};
use crate::window::proc::process_ref;
use std::fs;
use tempfile::tempdir;

fn own_process_ref() -> Result<ProcessRef, Box<dyn std::error::Error>> {
    process_ref(i64::from(std::process::id()))
        .ok_or_else(|| "no process ref for the test process".into())
}

fn stored_event_ids(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(parse_state(&fs::read_to_string(path)?)?
        .events
        .into_iter()
        .map(|event| event.id)
        .collect())
}

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
    let dead = ProcessRef {
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
