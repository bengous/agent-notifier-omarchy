use super::*;
use crate::event::{append_and_trim, empty_state, Agent};
use crate::intake::agents::profile;
use crate::test_fixtures::{base_event, base_pi_event, fixture_clock, workspace};

fn context(now: DateTime<Utc>) -> CaptureContext {
    CaptureContext {
        cwd: "/repo/dotfiles".to_owned(),
        project_path: "/repo/dotfiles".to_owned(),
        project_key: "/repo/dotfiles".to_owned(),
        branch_name: Some("main".to_owned()),
        workspace: None,
        now,
        random_id: "abcd".to_owned(),
        env_session_id: None,
    }
}

#[test]
fn parses_hook_json_with_snake_case_fields() {
    let input = parse_hook_input(r#"{"cwd":"/repo","session_id":"abc"}"#);
    assert_eq!(input.cwd.as_deref(), Some("/repo"));
    assert_eq!(input.session_id.as_deref(), Some("abc"));
}

#[test]
fn parses_hook_json_with_camel_case_aliases() {
    let input = parse_hook_input(
        r#"{"cwd":"/repo","sessionFile":"/repo/home/.pi/session.jsonl","leafId":"leaf-1"}"#,
    );
    assert_eq!(input.cwd.as_deref(), Some("/repo"));
    assert_eq!(
        input.session_file.as_deref(),
        Some("/repo/home/.pi/session.jsonl")
    );
    assert_eq!(input.leaf_id.as_deref(), Some("leaf-1"));
}

#[test]
fn a_json_carrying_both_spellings_fails_fast_as_a_duplicate_field() {
    let raw = r#"{"cwd":"/repo","session_id":"snake","sessionId":"camel"}"#;

    let error = serde_json::from_str::<HookInput>(raw).err();
    assert!(error
        .as_ref()
        .is_some_and(|error| error.to_string().contains("duplicate field")));

    let input = parse_hook_input(raw);
    assert_eq!(input.cwd, None);
    assert_eq!(input.session_id, None);
}

#[test]
fn builds_codex_event_shape() {
    let event = base_event();
    assert_eq!(event.id, "1778061600000-abcd");
    assert_eq!(event.agent, "codex");
    assert_eq!(event.kind, "main");
    assert_eq!(event.project_name, "dotfiles");
    assert_eq!(event.project_key.as_deref(), Some("/repo/dotfiles"));
    assert_eq!(event.branch_name.as_deref(), Some("main"));
    assert_eq!(event.session_id, "session-1");
    assert_eq!(event.status, EventStatus::Unread);
}

#[test]
fn builds_claude_event_shape() -> Result<(), Box<dyn std::error::Error>> {
    let event = build_event(
        profile(Agent::Claude),
        &HookInput {
            session_id: Some("claude-session-1".to_owned()),
            ..HookInput::default()
        },
        CaptureContext {
            workspace: Some(workspace(&base_event())?),
            ..context(fixture_clock())
        },
    );
    assert_eq!(event.agent, "claude");
    assert_eq!(event.kind, "main");
    assert_eq!(event.session_id, "claude-session-1");
    Ok(())
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

#[test]
fn the_session_id_falls_back_along_the_profile_chain() {
    let input = parse_hook_input(r#"{"leafId":"leaf-1"}"#);
    let event = build_event(profile(Agent::Pi), &input, context(fixture_clock()));

    assert_eq!(event.session_id, "leaf-1");
}

#[test]
fn without_any_session_source_the_session_id_is_unknown() {
    let event = build_event(
        profile(Agent::Codex),
        &HookInput::default(),
        context(fixture_clock()),
    );

    assert_eq!(event.session_id, "unknown");
}

#[test]
fn a_hypothetical_agent_is_a_profile_not_a_new_pipeline() {
    let hypothetical = Profile {
        id: "hypothetical",
        session_id_fields: &[SessionIdField::LeafId, SessionIdField::SessionId],
        session_id_env_var: "HYPOTHETICAL_SESSION_ID",
        title_source: None,
    };
    let input = parse_hook_input(r#"{"session_id":"s-1","leafId":"leaf-9"}"#);

    let event = build_event(&hypothetical, &input, context(fixture_clock()));

    assert_eq!(event.agent, "hypothetical");
    assert_eq!(event.session_id, "leaf-9");
    assert_eq!(event.kind, "main");
}

#[test]
fn the_environment_session_id_backstops_an_empty_chain() {
    let hypothetical = Profile {
        id: "hypothetical",
        session_id_fields: &[SessionIdField::SessionId],
        session_id_env_var: "HYPOTHETICAL_SESSION_ID",
        title_source: None,
    };

    let event = build_event(
        &hypothetical,
        &HookInput::default(),
        CaptureContext {
            env_session_id: Some("env-session".to_owned()),
            ..context(fixture_clock())
        },
    );

    assert_eq!(event.session_id, "env-session");
}
