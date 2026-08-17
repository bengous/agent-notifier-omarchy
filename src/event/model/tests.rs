use super::*;
use crate::display::event_label;
use crate::test_fixtures::{
    base_event, event_with_address, event_with_candidates, event_with_source_process, state_of,
    v1_state_json,
};

#[test]
fn rejects_invalid_state_shape() {
    let result = parse_state(r#"{"version":1,"events":[{"id":"missing-fields"}]}"#);
    assert!(result.is_err());
}

#[test]
fn parses_v1_state_without_a_project_key() -> Result<(), Box<dyn std::error::Error>> {
    let state = parse_state(&v1_state_json().to_string())?;

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
fn parses_v1_state_without_client_address() -> Result<(), Box<dyn std::error::Error>> {
    let state = parse_state(&v1_state_json().to_string())?;
    assert_eq!(state.events.len(), 1);
    Ok(())
}

#[test]
fn parses_v1_state_without_session_title() -> Result<(), Box<dyn std::error::Error>> {
    let state = parse_state(&v1_state_json().to_string())?;

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
fn a_parsed_v1_workspace_without_the_candidate_list_falls_back_to_the_primary(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = v1_state_json();
    raw["events"][0]["workspace"]["clientAddress"] = "0xbeef".into();
    let state = parse_state(&raw.to_string())?;
    let workspace = state
        .events
        .first()
        .and_then(|event| event.workspace.clone())
        .ok_or("missing workspace")?;

    assert_eq!(workspace.candidate_addresses(), ["0xbeef"]);
    Ok(())
}

#[test]
fn a_session_title_is_omitted_from_stored_state_when_absent() {
    let json = serde_json::to_string(&base_event()).unwrap_or_default();

    assert!(!json.contains("sessionTitle"));
}

#[test]
fn a_source_process_serializes_additively() -> Result<(), Box<dyn std::error::Error>> {
    let legacy_json = serde_json::to_string(&event_with_address("legacy", 1, "0xbeef"))?;
    assert!(!legacy_json.contains("sourceProcess"));

    let process = ProcessRef {
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
fn candidate_addresses_serialize_additively() -> Result<(), Box<dyn std::error::Error>> {
    let legacy_json = serde_json::to_string(&event_with_address("legacy", 1, "0xbeef"))?;
    assert!(!legacy_json.contains("clientAddresses"));

    let event = event_with_candidates("guessed", 4682, &["0xguess", "0xother"]);
    let value = serde_json::to_value(&event)?;
    assert_eq!(value["workspace"]["clientAddresses"][0], "0xguess");
    assert_eq!(value["workspace"]["clientAddresses"][1], "0xother");
    Ok(())
}
