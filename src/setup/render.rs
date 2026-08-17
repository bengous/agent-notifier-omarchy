use crate::setup::state::{HarnessReport, HarnessState, SetupReport};

const STATE_WIRED: &str = "wired";
const STATE_CONFIG_ABSENT: &str = "config absent";
const STATE_HOOK_ABSENT: &str = "hook absent";
const STATE_HOOK_STALE: &str = "hook stale";
const STATE_NOT_INSTALLED: &str = "not installed";

const BINARY_ON_PATH_YES: &str = "binary on PATH: yes";
const BINARY_ON_PATH_NO: &str = "binary on PATH: no";
const LISTENER_LIVE: &str = "listener: live";
const LISTENER_NOT_LIVE: &str = "listener: not live";

const INSTALL_POINTER: &str =
    "The agent-notifier binary is not on PATH. Install it first: README, section Install.";
const LISTENER_POINTER: &str =
    "The focused-window listener is not running. Start it from ~/.config/hypr/autostart.lua:";

// Every paste block below is byte for byte the matching README snippet;
// render/tests.rs holds the two together.
const LISTENER_BLOCK: &str =
    r#"o.exec_on_start("/usr/local/bin/agent-notifier watch-focused-window")"#;

pub(in crate::setup) const CLAUDE_HOOK_BLOCK: &str = r#"{
  "Stop": [
    {
      "hooks": [
        {
          "type": "command",
          "command": "agent-notifier claude-hook",
          "timeout": 5
        }
      ]
    }
  ]
}"#;

pub(in crate::setup) const CODEX_HOOK_BLOCK: &str = r#"[[hooks.Stop]]

[[hooks.Stop.hooks]]
command = "agent-notifier hook"
statusMessage = "Recording Codex completion"
timeout = 5
type = "command""#;

const PI_HOOK_BLOCK: &str = r#"import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { spawn } from "node:child_process";

const HOOK_COMMAND = "agent-notifier pi-hook";
const HOOK_TIMEOUT_MS = 5_000;

export default function (pi: ExtensionAPI) {
  pi.on("agent_end", (_event, ctx) => {
    if (process.env.PI_SUBAGENT_CHILD === "1") return;

    const [program, subcommand] = HOOK_COMMAND.split(" ");
    const child = spawn(program, [subcommand], {
      stdio: ["pipe", "ignore", "ignore"],
      timeout: HOOK_TIMEOUT_MS,
    });
    child.on("error", () => {});
    child.stdin.on("error", () => {});
    child.stdin.end(JSON.stringify({
      cwd: ctx.cwd,
      sessionFile: ctx.sessionManager.getSessionFile?.(),
      leafId: ctx.sessionManager.getLeafId?.(),
    }));
  });
}"#;

pub(crate) fn doctor_report(report: &SetupReport) -> String {
    let mut sections = vec![global_lines(report), summary_lines(report)];
    if !report.binary_on_path {
        sections.push(INSTALL_POINTER.to_owned());
    }
    sections.extend(report.harnesses.iter().filter_map(fix_section));
    if !report.listener_live {
        sections.push(format!("{LISTENER_POINTER}\n\n{LISTENER_BLOCK}"));
    }
    sections.join("\n\n")
}

fn global_lines(report: &SetupReport) -> String {
    format!(
        "{}\n{}",
        if report.binary_on_path {
            BINARY_ON_PATH_YES
        } else {
            BINARY_ON_PATH_NO
        },
        if report.listener_live {
            LISTENER_LIVE
        } else {
            LISTENER_NOT_LIVE
        }
    )
}

fn summary_lines(report: &SetupReport) -> String {
    report
        .harnesses
        .iter()
        .map(summary_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn summary_line(row: &HarnessReport) -> String {
    let label = state_label(row.state);
    match (row.state, &row.hook_command) {
        (HarnessState::Wired | HarnessState::HookStale, Some(command)) => {
            format!("{}: {label} ({command})", row.display_name)
        }
        _ => format!("{}: {label}", row.display_name),
    }
}

const fn state_label(state: HarnessState) -> &'static str {
    match state {
        HarnessState::HarnessAbsent => STATE_NOT_INSTALLED,
        HarnessState::ConfigAbsent => STATE_CONFIG_ABSENT,
        HarnessState::HookAbsent => STATE_HOOK_ABSENT,
        HarnessState::HookStale => STATE_HOOK_STALE,
        HarnessState::Wired => STATE_WIRED,
    }
}

fn fix_section(row: &HarnessReport) -> Option<String> {
    let (intro, block) = fix_recipe(&row.harness)?;
    match row.state {
        HarnessState::ConfigAbsent | HarnessState::HookAbsent => Some(format!(
            "{}: {intro} {}:\n\n{block}",
            row.display_name, row.config_path
        )),
        HarnessState::HookStale => Some(format!(
            "{}: the hook command does not resolve: {}\nReplace it in {}:\n\n{block}",
            row.display_name,
            row.hook_command.as_deref().unwrap_or_default(),
            row.config_path
        )),
        HarnessState::HarnessAbsent | HarnessState::Wired => None,
    }
}

fn fix_recipe(harness: &str) -> Option<(&'static str, &'static str)> {
    match harness {
        "claude" => Some(("merge this into \"hooks\" in", CLAUDE_HOOK_BLOCK)),
        "codex" => Some(("add this to", CODEX_HOOK_BLOCK)),
        "pi" => Some(("create", PI_HOOK_BLOCK)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
