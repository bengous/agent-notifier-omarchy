use super::*;
use crate::test_fixtures::{base_event, workspace};
use tempfile::tempdir;

#[test]
fn parses_stop_hook_json() {
    let input = parse_stop_hook_input(r#"{"cwd":"/repo","session_id":"abc"}"#);
    assert_eq!(input.cwd.as_deref(), Some("/repo"));
    assert_eq!(input.session_id.as_deref(), Some("abc"));
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
    let event = build_stop_event(
        "claude",
        &StopHookInput {
            cwd: Some("/repo/dotfiles".to_owned()),
            session_id: Some("claude-session-1".to_owned()),
            session_id_camel: None,
            transcript_path: None,
        },
        "/repo/dotfiles".to_owned(),
        "/repo/dotfiles".to_owned(),
        "/repo/dotfiles".to_owned(),
        Some("main".to_owned()),
        Some(workspace(&base_event())?),
        DateTime::from_timestamp_millis(1_778_061_600_000).unwrap_or_else(Utc::now),
        "abcd",
    );
    assert_eq!(event.agent, "claude");
    assert_eq!(event.kind, "main");
    assert_eq!(event.session_id, "claude-session-1");
    Ok(())
}

#[test]
fn a_worktree_common_dir_resolves_to_the_main_repository() {
    assert_eq!(
        main_repository_root("/repo/dotfiles/.git").as_deref(),
        Some("/repo/dotfiles")
    );
}

#[test]
fn a_bare_repository_path_stays_the_key() {
    assert_eq!(
        main_repository_root("/repo/dotfiles.git").as_deref(),
        Some("/repo/dotfiles.git")
    );
}

#[test]
fn a_git_directory_without_a_parent_yields_no_key() {
    assert_eq!(main_repository_root(""), None);
    assert_eq!(main_repository_root(".git"), None);
}

#[test]
fn outside_a_repository_the_key_is_the_project_path() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;

    assert_eq!(
        repository_key(&dir.path().to_string_lossy(), "/repo/dotfiles"),
        "/repo/dotfiles"
    );
    Ok(())
}
