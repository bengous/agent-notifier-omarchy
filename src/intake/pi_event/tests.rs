use super::*;
use crate::event::{append_and_trim, empty_state};
use crate::test_fixtures::{base_event, base_pi_event, workspace};

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
