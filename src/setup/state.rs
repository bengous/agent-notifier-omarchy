use serde::Serialize;

use crate::event::Agent;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum HarnessState {
    HarnessAbsent,
    ConfigAbsent,
    HookAbsent,
    HookStale,
    Wired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HarnessFacts {
    pub(crate) agent: Agent,
    pub(crate) config_path: String,
    pub(crate) harness_on_path: bool,
    pub(crate) config_exists: bool,
    pub(crate) hook_command: Option<String>,
    pub(crate) hook_command_resolves: bool,
}

pub(crate) fn harness_state(facts: &HarnessFacts) -> HarnessState {
    if !facts.harness_on_path {
        return HarnessState::HarnessAbsent;
    }
    if !facts.config_exists {
        return HarnessState::ConfigAbsent;
    }
    match &facts.hook_command {
        None => HarnessState::HookAbsent,
        Some(_) if !facts.hook_command_resolves => HarnessState::HookStale,
        Some(_) => HarnessState::Wired,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HarnessReport {
    pub(crate) harness: String,
    pub(crate) display_name: String,
    pub(crate) state: HarnessState,
    pub(crate) config_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hook_command: Option<String>,
}

pub(crate) fn harness_report(facts: &HarnessFacts) -> HarnessReport {
    HarnessReport {
        harness: facts.agent.id().to_owned(),
        display_name: facts.agent.display_name().to_owned(),
        state: harness_state(facts),
        config_path: facts.config_path.clone(),
        hook_command: facts.hook_command.clone(),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetupReport {
    pub(crate) version: u8,
    pub(crate) binary_on_path: bool,
    pub(crate) listener_live: bool,
    pub(crate) harnesses: Vec<HarnessReport>,
}

/// The listener stays out of readiness: without it, mark-read-on-focus
/// degrades, but completions still reach the widget.
// expect(dead_code) dies with the first production caller: the display summary.
#[cfg_attr(not(test), expect(dead_code))]
pub(crate) fn is_ready(report: &SetupReport) -> bool {
    report.binary_on_path
        && report
            .harnesses
            .iter()
            .any(|harness| harness.state == HarnessState::Wired)
        && report
            .harnesses
            .iter()
            .all(|harness| harness.state != HarnessState::HookStale)
}

#[cfg(test)]
mod tests;
