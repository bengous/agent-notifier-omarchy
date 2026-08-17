use super::*;

#[test]
fn a_claude_event_displays_as_claude() {
    assert_eq!(Agent::Claude.display_name(), "Claude");
}

#[test]
fn a_codex_event_displays_as_codex() {
    assert_eq!(Agent::Codex.display_name(), "Codex");
}

#[test]
fn a_pi_event_displays_as_pi() {
    assert_eq!(Agent::Pi.display_name(), "Pi");
}

#[test]
fn every_agent_id_round_trips_through_from_id() {
    for agent in [Agent::Claude, Agent::Codex, Agent::Pi] {
        assert_eq!(Agent::from_id(agent.id()), Some(agent));
    }
    assert_eq!(Agent::from_id("gemini"), None);
    assert_eq!(Agent::from_id(""), None);
}
