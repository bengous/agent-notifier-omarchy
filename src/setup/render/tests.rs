use super::*;
use crate::event::Agent;
use crate::test_fixtures::{nothing_installed_probe, probe_row, wired_probe};

const README: &str = include_str!("../../../README.md");

#[test]
fn the_readme_documents_every_doctor_state_literal() {
    for literal in [
        STATE_WIRED,
        STATE_CONFIG_ABSENT,
        STATE_HOOK_ABSENT,
        STATE_HOOK_STALE,
        STATE_NOT_INSTALLED,
        BINARY_ON_PATH_NO,
        LISTENER_NOT_LIVE,
    ] {
        assert!(
            README.contains(&format!("`{literal}`")),
            "the README does not document `{literal}`"
        );
    }
}

#[test]
fn the_readme_hook_blocks_match_the_doctor_paste_blocks() {
    for (name, block) in [
        ("Claude", CLAUDE_HOOK_BLOCK),
        ("Codex", CODEX_HOOK_BLOCK),
        ("Pi", PI_HOOK_BLOCK),
        ("listener", LISTENER_BLOCK),
    ] {
        assert!(
            README.contains(block),
            "the README {name} snippet drifted from the doctor paste block"
        );
    }
}

#[test]
fn a_wired_report_prints_the_summary_without_paste_blocks() {
    assert_eq!(
        doctor_report(&wired_probe()),
        "binary on PATH: yes\nlistener: live\n\n\
         Claude: wired (agent-notifier claude-hook)\n\
         Codex: wired (agent-notifier hook)\n\
         Pi: wired (agent-notifier pi-hook)"
    );
}

#[test]
fn a_bare_machine_report_points_to_the_install_section() {
    let output = doctor_report(&nothing_installed_probe());

    assert!(output.contains(BINARY_ON_PATH_NO));
    assert!(output.contains(INSTALL_POINTER));
    assert!(output.contains("Claude: not installed"));
    assert!(!output.contains(CLAUDE_HOOK_BLOCK));
    assert!(output.contains(LISTENER_POINTER));
    assert!(output.contains(LISTENER_BLOCK));
}

#[test]
fn a_fixable_harness_gets_its_readme_block_to_paste() {
    let report = SetupReport {
        harnesses: vec![
            probe_row(Agent::Claude, HarnessState::ConfigAbsent, None),
            probe_row(Agent::Codex, HarnessState::HookAbsent, None),
            probe_row(
                Agent::Pi,
                HarnessState::Wired,
                Some("agent-notifier pi-hook"),
            ),
        ],
        ..wired_probe()
    };

    let output = doctor_report(&report);

    assert!(
        output.contains("Claude: merge this into \"hooks\" in /repo/home/.claude/settings.json:")
    );
    assert!(output.contains(CLAUDE_HOOK_BLOCK));
    assert!(output.contains("Codex: add this to /repo/home/.codex/config.toml:"));
    assert!(output.contains(CODEX_HOOK_BLOCK));
    assert!(output.contains("Pi: wired (agent-notifier pi-hook)"));
    assert!(!output.contains(PI_HOOK_BLOCK));
}

#[test]
fn a_stale_hook_report_names_the_pending_command() {
    let report = SetupReport {
        harnesses: vec![probe_row(
            Agent::Codex,
            HarnessState::HookStale,
            Some("/gone/agent-notifier hook"),
        )],
        ..wired_probe()
    };

    let output = doctor_report(&report);

    assert!(output.contains("Codex: hook stale (/gone/agent-notifier hook)"));
    assert!(output.contains(
        "Codex: the hook command does not resolve: /gone/agent-notifier hook\n\
         Replace it in /repo/home/.codex/config.toml:"
    ));
    assert!(output.contains(CODEX_HOOK_BLOCK));
}
