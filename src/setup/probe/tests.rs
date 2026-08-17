use super::*;

#[test]
fn the_claude_hook_command_is_read_from_the_stop_hooks() {
    let raw = r#"{"hooks":{"Stop":[
        {"hooks":[{"type":"command","command":"other-tool stop"}]},
        {"hooks":[{"type":"command","command":"~/.local/bin/agent-notifier claude-hook","timeout":5}]}
    ]}}"#;

    assert_eq!(
        claude_hook_command_from(raw),
        Some("~/.local/bin/agent-notifier claude-hook".to_owned())
    );
}

#[test]
fn a_settings_file_without_the_hook_yields_no_command() {
    assert_eq!(
        claude_hook_command_from(r#"{"hooks":{"Stop":[{"hooks":[{"command":"other"}]}]}}"#),
        None
    );
    assert_eq!(claude_hook_command_from(r#"{"model":"opus"}"#), None);
}

#[test]
fn an_unparsable_settings_file_falls_back_to_the_marker_scan() {
    let raw = r#"{"hooks": {"Stop": [{"hooks": [{"command": "agent-notifier claude-hook"},]}]}"#;

    assert_eq!(
        claude_hook_command_from(raw),
        Some("agent-notifier claude-hook".to_owned())
    );
}

#[test]
fn the_codex_hook_command_is_read_from_a_command_line() {
    let raw = "hooks = true\n\n[[hooks.Stop]]\n\n[[hooks.Stop.hooks]]\ncommand = \"agent-notifier hook\"\ntimeout = 5\n";

    assert_eq!(
        codex_hook_command_from(raw),
        Some("agent-notifier hook".to_owned())
    );
}

#[test]
fn a_commented_codex_command_line_is_ignored() {
    assert_eq!(
        codex_hook_command_from("# command = \"agent-notifier hook\"\n"),
        None
    );
}

#[test]
fn a_codex_command_for_another_tool_is_ignored() {
    assert_eq!(
        codex_hook_command_from(
            "command = \"bun /home/user/.codex/hooks/safety-hooks.ts guard\"\n"
        ),
        None
    );
}

#[test]
fn the_pi_marker_is_found_with_its_path_prefix() {
    assert_eq!(
        marker_command(
            "const HOOK_COMMAND = \"agent-notifier pi-hook\";",
            "pi-hook"
        ),
        Some("agent-notifier pi-hook".to_owned())
    );
    assert_eq!(
        marker_command(
            "spawn(\"/usr/local/bin/agent-notifier pi-hook\")",
            "pi-hook"
        ),
        Some("/usr/local/bin/agent-notifier pi-hook".to_owned())
    );
    assert_eq!(
        marker_command("pi.on(\"agent_end\", noop)", "pi-hook"),
        None
    );
}

#[test]
fn codex_home_overrides_the_default_codex_dir() {
    assert_eq!(
        codex_config_path_from(
            Some(OsStr::new("/custom/codex")),
            Some(Path::new("/home/u"))
        ),
        PathBuf::from("/custom/codex/config.toml")
    );
    assert_eq!(
        codex_config_path_from(Some(OsStr::new("")), Some(Path::new("/home/u"))),
        PathBuf::from("/home/u/.codex/config.toml")
    );
    assert_eq!(
        codex_config_path_from(None, Some(Path::new("/home/u"))),
        PathBuf::from("/home/u/.codex/config.toml")
    );
}

#[test]
fn a_bare_hook_program_resolves_through_path_lookup() {
    let on_path = |program: &str| program == "agent-notifier";
    let exists = |_: &Path| false;

    assert!(hook_program_resolves(
        "agent-notifier claude-hook",
        None,
        &on_path,
        &exists
    ));
    assert!(!hook_program_resolves(
        "gone-binary claude-hook",
        None,
        &on_path,
        &exists
    ));
    assert!(!hook_program_resolves("", None, &on_path, &exists));
}

#[test]
fn an_absolute_hook_program_resolves_through_its_path() {
    let on_path = |_: &str| false;
    let exists = |path: &Path| path == Path::new("/usr/local/bin/agent-notifier");

    assert!(hook_program_resolves(
        "/usr/local/bin/agent-notifier hook",
        None,
        &on_path,
        &exists
    ));
    assert!(!hook_program_resolves(
        "/gone/agent-notifier hook",
        None,
        &on_path,
        &exists
    ));
}

#[test]
fn a_tilde_hook_program_resolves_against_home() {
    let on_path = |_: &str| false;
    let exists = |path: &Path| path == Path::new("/home/u/.local/bin/agent-notifier");

    assert!(hook_program_resolves(
        "~/.local/bin/agent-notifier claude-hook",
        Some(Path::new("/home/u")),
        &on_path,
        &exists
    ));
}
