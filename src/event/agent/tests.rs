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
