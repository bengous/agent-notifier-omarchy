use super::*;

#[test]
fn a_claude_event_displays_as_claude() {
    assert_eq!(Agent::from_id("claude").display_name(), "Claude");
}

#[test]
fn a_pi_event_displays_as_pi() {
    assert_eq!(Agent::from_id("pi").display_name(), "Pi");
}

#[test]
fn an_unknown_agent_id_displays_as_codex() {
    assert_eq!(Agent::from_id("mystery").display_name(), "Codex");
}
