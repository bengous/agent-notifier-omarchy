use super::*;
use crate::test_fixtures::{nothing_installed_probe, probe_row, wired_probe};
use std::error::Error;

fn wired_facts() -> HarnessFacts {
    HarnessFacts {
        agent: Agent::Claude,
        config_path: "/repo/home/.claude/settings.json".to_owned(),
        harness_on_path: true,
        config_exists: true,
        hook_command: Some("agent-notifier claude-hook".to_owned()),
        hook_command_resolves: true,
    }
}

#[test]
fn a_harness_off_path_is_harness_absent() {
    assert_eq!(
        harness_state(&HarnessFacts {
            harness_on_path: false,
            ..wired_facts()
        }),
        HarnessState::HarnessAbsent
    );
}

#[test]
fn a_missing_config_is_config_absent() {
    assert_eq!(
        harness_state(&HarnessFacts {
            config_exists: false,
            ..wired_facts()
        }),
        HarnessState::ConfigAbsent
    );
}

#[test]
fn a_config_without_the_hook_is_hook_absent() {
    assert_eq!(
        harness_state(&HarnessFacts {
            hook_command: None,
            ..wired_facts()
        }),
        HarnessState::HookAbsent
    );
}

#[test]
fn a_hook_command_that_does_not_resolve_is_hook_stale() {
    assert_eq!(
        harness_state(&HarnessFacts {
            hook_command_resolves: false,
            ..wired_facts()
        }),
        HarnessState::HookStale
    );
}

#[test]
fn a_resolving_hook_in_an_existing_config_is_wired() {
    assert_eq!(harness_state(&wired_facts()), HarnessState::Wired);
}

#[test]
fn readiness_needs_one_wired_harness_and_no_stale_hook() {
    assert!(is_ready(&wired_probe()));
    assert!(!is_ready(&nothing_installed_probe()));

    let one_wired = SetupReport {
        harnesses: vec![
            probe_row(
                Agent::Claude,
                HarnessState::Wired,
                Some("agent-notifier claude-hook"),
            ),
            probe_row(Agent::Codex, HarnessState::HookAbsent, None),
            probe_row(Agent::Pi, HarnessState::HarnessAbsent, None),
        ],
        ..wired_probe()
    };
    assert!(is_ready(&one_wired));

    let one_stale = SetupReport {
        harnesses: vec![
            probe_row(
                Agent::Claude,
                HarnessState::Wired,
                Some("agent-notifier claude-hook"),
            ),
            probe_row(
                Agent::Codex,
                HarnessState::HookStale,
                Some("/gone/agent-notifier hook"),
            ),
            probe_row(Agent::Pi, HarnessState::HarnessAbsent, None),
        ],
        ..wired_probe()
    };
    assert!(!is_ready(&one_stale));

    assert!(!is_ready(&SetupReport {
        binary_on_path: false,
        ..wired_probe()
    }));
}

#[test]
fn a_dead_listener_does_not_break_readiness() {
    assert!(is_ready(&SetupReport {
        listener_live: false,
        ..wired_probe()
    }));
}

#[test]
fn harness_states_serialize_kebab_case() -> Result<(), Box<dyn Error>> {
    let spellings = [
        (HarnessState::HarnessAbsent, "harness-absent"),
        (HarnessState::ConfigAbsent, "config-absent"),
        (HarnessState::HookAbsent, "hook-absent"),
        (HarnessState::HookStale, "hook-stale"),
        (HarnessState::Wired, "wired"),
    ];
    for (state, spelling) in spellings {
        assert_eq!(serde_json::to_value(state)?, serde_json::json!(spelling));
    }
    Ok(())
}

#[test]
fn the_setup_report_serializes_the_documented_shape() -> Result<(), Box<dyn Error>> {
    let report = SetupReport {
        version: 1,
        binary_on_path: true,
        listener_live: false,
        harnesses: vec![probe_row(
            Agent::Claude,
            HarnessState::Wired,
            Some("agent-notifier claude-hook"),
        )],
    };

    assert_eq!(
        serde_json::to_value(&report)?,
        serde_json::json!({
            "version": 1,
            "binaryOnPath": true,
            "listenerLive": false,
            "harnesses": [{
                "harness": "claude",
                "displayName": "Claude",
                "state": "wired",
                "configPath": "/repo/home/.claude/settings.json",
                "hookCommand": "agent-notifier claude-hook"
            }]
        })
    );
    Ok(())
}

#[test]
fn a_hook_absent_row_serializes_without_a_hook_command() -> Result<(), Box<dyn Error>> {
    let row = serde_json::to_value(probe_row(Agent::Pi, HarnessState::HookAbsent, None))?;
    let object = row.as_object().ok_or("row is not an object")?;

    assert!(!object.contains_key("hookCommand"));
    Ok(())
}
