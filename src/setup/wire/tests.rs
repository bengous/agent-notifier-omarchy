use super::*;
use std::error::Error;

const CLAUDE_CONFIG: &str = "/repo/home/.claude/settings.json";
const CODEX_CONFIG: &str = "/repo/home/.codex/config.toml";

fn everything_resolves(_command: &str) -> bool {
    true
}

fn only_the_canonical_commands_resolve(command: &str) -> bool {
    command == "agent-notifier claude-hook" || command == "agent-notifier hook"
}

fn config_path(target: WireTarget) -> &'static str {
    match target {
        WireTarget::Claude => CLAUDE_CONFIG,
        WireTarget::Codex => CODEX_CONFIG,
    }
}

fn planned(
    target: WireTarget,
    action: WireAction,
    existing: Option<&str>,
) -> Result<WirePlan, WireError> {
    planned_with(target, action, existing, true, &everything_resolves)
}

fn planned_with(
    target: WireTarget,
    action: WireAction,
    existing: Option<&str>,
    harness_on_path: bool,
    resolves: &dyn Fn(&str) -> bool,
) -> Result<WirePlan, WireError> {
    let spec = target_spec(target);
    wire_plan(&WirePlanInput {
        spec: &spec,
        action,
        config_path: config_path(target),
        harness_on_path,
        existing,
        resolves,
    })
}

fn edited(result: Result<String, EditRefusal>) -> Result<String, Box<dyn Error>> {
    result.map_err(|refusal| format!("the editor refused: {refusal:?}").into())
}

fn written(plan: Result<WirePlan, WireError>) -> Result<String, Box<dyn Error>> {
    match plan? {
        WirePlan::Write(text) => Ok(text),
        WirePlan::AlreadyDone(change) => Err(format!("expected a write, got {change:?}").into()),
    }
}

fn claude_settings_after(
    action: WireAction,
    existing: Option<&str>,
) -> Result<Value, Box<dyn Error>> {
    let text = written(planned(WireTarget::Claude, action, existing))?;
    assert!(
        text.ends_with('\n'),
        "the settings file needs a final newline"
    );
    Ok(serde_json::from_str(&text)?)
}

#[test]
fn an_empty_claude_settings_gains_the_stop_hook() -> Result<(), Box<dyn Error>> {
    let settings = claude_settings_after(WireAction::Wire, None)?;

    assert_eq!(
        settings["hooks"]["Stop"][0]["hooks"][0]["command"],
        "agent-notifier claude-hook"
    );
    assert_eq!(settings["hooks"]["Stop"][0]["hooks"][0]["timeout"], 5);
    Ok(())
}

#[test]
fn existing_claude_settings_keep_their_other_keys_and_gain_the_hook() -> Result<(), Box<dyn Error>>
{
    let existing = r#"{"model":"opus","hooks":{"PreToolUse":[{"matcher":"Bash"}]}}"#;

    let settings = claude_settings_after(WireAction::Wire, Some(existing))?;

    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["hooks"]["PreToolUse"][0]["matcher"], "Bash");
    assert_eq!(
        settings["hooks"]["Stop"][0]["hooks"][0]["command"],
        "agent-notifier claude-hook"
    );
    Ok(())
}

#[test]
fn a_stale_claude_hook_command_is_replaced_not_duplicated() -> Result<(), Box<dyn Error>> {
    let existing =
        r#"{"hooks":{"Stop":[{"hooks":[{"command":"/gone/agent-notifier claude-hook"}]}]}}"#;

    let text = written(planned_with(
        WireTarget::Claude,
        WireAction::Wire,
        Some(existing),
        true,
        &only_the_canonical_commands_resolve,
    ))?;

    let settings = serde_json::from_str::<Value>(&text)?;
    let matchers = settings["hooks"]["Stop"]
        .as_array()
        .ok_or("Stop is not an array")?;
    assert_eq!(matchers.len(), 1);
    assert_eq!(
        matchers[0]["hooks"].as_array().map(Vec::len),
        Some(1),
        "the stale command must be replaced, not joined"
    );
    assert_eq!(
        matchers[0]["hooks"][0]["command"],
        "agent-notifier claude-hook"
    );
    Ok(())
}

#[test]
fn rewiring_already_wired_claude_settings_changes_nothing() -> Result<(), Box<dyn Error>> {
    let once = edited(claude_settings_with_hook(None))?;

    assert_eq!(edited(claude_settings_with_hook(Some(&once)))?, once);
    Ok(())
}

#[test]
fn unparsable_claude_settings_refuse_the_edit() {
    let truncated = r#"{"model": "opus",}"#;
    let with_marker =
        r#"{"hooks": {"Stop": [{"hooks": [{"command": "agent-notifier claude-hook"},]}]}"#;

    assert!(matches!(
        planned(WireTarget::Claude, WireAction::Wire, Some(truncated)),
        Err(WireError::ConfigUnparsable { .. })
    ));
    assert!(matches!(
        planned(WireTarget::Claude, WireAction::Remove, Some(with_marker)),
        Err(WireError::ConfigUnparsable { .. })
    ));
}

#[test]
fn a_claude_settings_file_that_is_not_an_object_refuses_the_edit() {
    assert!(matches!(
        planned(WireTarget::Claude, WireAction::Wire, Some("[1, 2]")),
        Err(WireError::ConfigUnparsable { .. })
    ));
}

#[test]
fn removing_the_claude_hook_leaves_other_stop_hooks_in_place() -> Result<(), Box<dyn Error>> {
    let existing = r#"{"hooks":{"Stop":[{"hooks":[
        {"command":"other-tool stop"},
        {"command":"agent-notifier claude-hook"}
    ]}]}}"#;

    let settings = claude_settings_after(WireAction::Remove, Some(existing))?;

    assert_eq!(
        settings["hooks"]["Stop"][0]["hooks"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        settings["hooks"]["Stop"][0]["hooks"][0]["command"],
        "other-tool stop"
    );
    Ok(())
}

#[test]
fn removing_the_claude_hook_drops_the_emptied_containers() -> Result<(), Box<dyn Error>> {
    let existing = r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"command":"agent-notifier claude-hook"}]}]}}"#;

    let settings = claude_settings_after(WireAction::Remove, Some(existing))?;

    assert_eq!(settings, serde_json::json!({"model": "opus"}));
    Ok(())
}

#[test]
fn removing_the_claude_hook_keeps_the_other_hook_events() -> Result<(), Box<dyn Error>> {
    let existing = r#"{"hooks":{"PreToolUse":[{"matcher":"Bash"}],"Stop":[{"hooks":[{"command":"agent-notifier claude-hook"}]}]}}"#;

    let settings = claude_settings_after(WireAction::Remove, Some(existing))?;

    assert_eq!(
        settings,
        serde_json::json!({"hooks": {"PreToolUse": [{"matcher": "Bash"}]}})
    );
    Ok(())
}

#[test]
fn the_claude_hook_block_constant_parses_as_json() -> Result<(), Box<dyn Error>> {
    let matcher = canonical_claude_matcher().ok_or("the Claude paste block stopped parsing")?;

    assert_eq!(matcher["hooks"][0]["command"], "agent-notifier claude-hook");
    Ok(())
}

#[test]
fn every_paste_block_carries_the_canonical_command() {
    for target in [WireTarget::Claude, WireTarget::Codex] {
        let spec = target_spec(target);
        assert!(
            spec.block.contains(spec.canonical_command),
            "the {} paste block dropped {}",
            spec.agent.display_name(),
            spec.canonical_command
        );
    }
}

#[test]
fn an_existing_codex_config_gains_the_block_appended_after_a_blank_line(
) -> Result<(), Box<dyn Error>> {
    let text = written(planned(
        WireTarget::Codex,
        WireAction::Wire,
        Some("model = \"gpt\"\n"),
    ))?;

    assert_eq!(text, format!("model = \"gpt\"\n\n{CODEX_HOOK_BLOCK}\n"));
    Ok(())
}

#[test]
fn an_absent_codex_config_becomes_the_block_alone() -> Result<(), Box<dyn Error>> {
    let text = written(planned(WireTarget::Codex, WireAction::Wire, None))?;

    assert_eq!(text, format!("{CODEX_HOOK_BLOCK}\n"));
    Ok(())
}

#[test]
fn the_codex_block_is_never_appended_twice() -> Result<(), Box<dyn Error>> {
    let once = edited(codex_config_with_hook(Some("model = \"gpt\"\n")))?;

    assert_eq!(edited(codex_config_with_hook(Some(&once)))?, once);
    Ok(())
}

#[test]
fn a_hand_edited_codex_block_refuses_the_edit() {
    let existing = "[[hooks.Stop.hooks]]\ncommand = \"/opt/agent-notifier hook\"\n";

    for action in [WireAction::Wire, WireAction::Remove] {
        assert!(matches!(
            planned_with(
                WireTarget::Codex,
                action,
                Some(existing),
                true,
                &only_the_canonical_commands_resolve
            ),
            Err(WireError::ForeignContent { .. })
        ));
    }
}

#[test]
fn removing_the_codex_block_restores_the_previous_bytes() -> Result<(), Box<dyn Error>> {
    let original = "model = \"gpt\"\n";

    let wired = edited(codex_config_with_hook(Some(original)))?;

    assert_eq!(edited(codex_config_without_hook(Some(&wired)))?, original);
    Ok(())
}

#[test]
fn an_already_wired_config_is_left_alone() -> Result<(), Box<dyn Error>> {
    let wired = edited(codex_config_with_hook(None))?;

    assert_eq!(
        planned(WireTarget::Codex, WireAction::Wire, Some(&wired))?,
        WirePlan::AlreadyDone(WireChange::AlreadyWired)
    );
    Ok(())
}

#[test]
fn removing_an_absent_hook_writes_nothing() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        planned(
            WireTarget::Codex,
            WireAction::Remove,
            Some("model = \"gpt\"\n")
        )?,
        WirePlan::AlreadyDone(WireChange::NothingToRemove)
    );
    assert_eq!(
        planned(WireTarget::Claude, WireAction::Remove, None)?,
        WirePlan::AlreadyDone(WireChange::NothingToRemove)
    );
    Ok(())
}

#[test]
fn install_on_an_absent_harness_is_refused() {
    assert_eq!(
        planned_with(
            WireTarget::Claude,
            WireAction::Wire,
            None,
            false,
            &everything_resolves
        ),
        Err(WireError::HarnessAbsent { harness: "Claude" })
    );
}

#[test]
fn install_without_the_binary_on_path_is_refused_before_any_write() {
    assert_eq!(
        planned_with(
            WireTarget::Codex,
            WireAction::Wire,
            None,
            true,
            &|_command| false
        ),
        Err(WireError::BinaryUnresolvable)
    );
}

#[test]
fn remove_works_even_when_the_harness_left_the_path() -> Result<(), Box<dyn Error>> {
    let wired = edited(codex_config_with_hook(None))?;

    let plan = planned_with(
        WireTarget::Codex,
        WireAction::Remove,
        Some(&wired),
        false,
        &everything_resolves,
    );

    assert_eq!(written(plan)?, "");
    Ok(())
}

#[test]
fn the_probe_extractor_reads_back_every_freshly_wired_config() -> Result<(), Box<dyn Error>> {
    for target in [WireTarget::Claude, WireTarget::Codex] {
        let spec = target_spec(target);
        let text = edited((spec.with_hook)(None))?;

        assert_eq!(
            (spec.extract)(&text).as_deref(),
            Some(spec.canonical_command)
        );
        assert!(written_verifies(
            &spec,
            WireAction::Wire,
            &text,
            &everything_resolves
        ));
        assert!(written_verifies(
            &spec,
            WireAction::Remove,
            &edited((spec.without_hook)(Some(&text)))?,
            &everything_resolves
        ));
    }
    Ok(())
}

#[test]
fn a_written_hook_that_does_not_resolve_fails_the_validation() -> Result<(), Box<dyn Error>> {
    let spec = target_spec(WireTarget::Claude);
    let text = edited((spec.with_hook)(None))?;

    assert!(!written_verifies(
        &spec,
        WireAction::Wire,
        &text,
        &|_command| { false }
    ));
    Ok(())
}

#[test]
fn each_wire_change_renders_its_success_line() {
    let line = |target, change| {
        wire_line(
            target,
            &WireOutcome {
                config_path: config_path(target).to_owned(),
                change,
            },
        )
    };

    assert_eq!(
        line(WireTarget::Claude, WireChange::Wired),
        format!("Claude wired ({CLAUDE_CONFIG})")
    );
    assert_eq!(
        line(WireTarget::Claude, WireChange::AlreadyWired),
        format!("Claude already wired ({CLAUDE_CONFIG})")
    );
    assert_eq!(
        line(WireTarget::Codex, WireChange::Removed),
        format!("Codex hook removed ({CODEX_CONFIG})")
    );
    assert_eq!(
        line(WireTarget::Codex, WireChange::NothingToRemove),
        format!("Codex hook already absent ({CODEX_CONFIG})")
    );
}

#[test]
fn each_wire_error_renders_its_operator_message() {
    assert_eq!(
        WireError::HarnessAbsent { harness: "Claude" }.to_string(),
        "Claude is not on PATH; install it first"
    );
    assert_eq!(
        WireError::HarnessDirAbsent {
            harness: "Codex",
            dir: "/repo/home/.codex".to_owned()
        }
        .to_string(),
        "/repo/home/.codex does not exist; run Codex once, then run this again"
    );
    assert_eq!(
        WireError::BinaryUnresolvable.to_string(),
        "agent-notifier is not on PATH; install the binary first (README, section Install)"
    );
    assert_eq!(
        WireError::ConfigUnparsable {
            path: CLAUDE_CONFIG.to_owned(),
            block: "<block>"
        }
        .to_string(),
        format!("cannot parse {CLAUDE_CONFIG}; add this block by hand:\n\n<block>")
    );
    assert!(WireError::ForeignContent {
        path: CODEX_CONFIG.to_owned(),
        block: "<block>"
    }
    .to_string()
    .starts_with(&format!(
        "{CODEX_CONFIG} holds a hand-edited agent-notifier block; remove it yourself"
    )));
    assert_eq!(
        WireError::LockHeld {
            path: "/run/user/1000/agent-notifier/setup.lock".to_owned(),
            pid: "4242".to_owned()
        }
        .to_string(),
        "another agent-notifier setup holds /run/user/1000/agent-notifier/setup.lock (pid 4242)"
    );
    assert_eq!(
        WireError::HookDidNotVerify {
            path: CLAUDE_CONFIG.to_owned()
        }
        .to_string(),
        format!(
            "wrote {CLAUDE_CONFIG} but the hook does not read back; the previous file is restored"
        )
    );
}

#[test]
fn only_the_wireable_harnesses_have_a_target() {
    assert_eq!(
        WireTarget::from_harness_id("claude"),
        Some(WireTarget::Claude)
    );
    assert_eq!(
        WireTarget::from_harness_id("codex"),
        Some(WireTarget::Codex)
    );
    assert_eq!(WireTarget::from_harness_id("pi"), None);
    assert_eq!(WireTarget::from_harness_id("gemini"), None);
}
