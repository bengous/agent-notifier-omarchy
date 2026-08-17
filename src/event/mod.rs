pub(crate) mod store;

use serde::{Deserialize, Serialize};

pub(crate) const STATE_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EventStatus {
    Unread,
    Read,
}

/// `start_time` (jiffies since boot, from `/proc/<pid>/stat`) makes the
/// reference immune to pid recycling.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProcessRef {
    pub(crate) pid: i64,
    #[serde(rename = "startTime")]
    pub(crate) start_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct SourceWindow {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) monitor: String,
    #[serde(rename = "clientPid")]
    pub(crate) client_pid: i64,
    #[serde(
        default,
        rename = "clientAddress",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) client_address: Option<String>,
    /// Every window that can be the source, best guess first. A single-process
    /// terminal gives all its windows one pid, so the true source window is not
    /// knowable at capture time. Invariant: the first entry is `client_address`.
    #[serde(
        default,
        rename = "clientAddresses",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub(crate) client_addresses: Vec<String>,
    /// The window's own shell in the hook's process chain: the per-window
    /// liveness anchor a shared-pid terminal cannot provide.
    #[serde(
        default,
        rename = "sourceProcess",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) source_process: Option<ProcessRef>,
    pub(crate) title: String,
    /// Keeps keys written by a newer binary alive across a rewrite by this one.
    /// This only covers additive-key schema evolution: a new `EventStatus`
    /// variant, a changed type, or a `version` bump still makes an old binary
    /// quarantine the file. No `i128`/`u128` fields anywhere in the state:
    /// serde's flatten buffering does not carry them.
    #[serde(flatten)]
    pub(crate) extra: serde_json::Map<String, serde_json::Value>,
}

impl SourceWindow {
    /// Legacy states carry only the primary address.
    pub(crate) fn candidate_addresses(&self) -> Vec<&str> {
        if self.client_addresses.is_empty() {
            self.client_address.as_deref().into_iter().collect()
        } else {
            self.client_addresses.iter().map(String::as_str).collect()
        }
    }

    /// A certain source: the candidate set is exactly this one window.
    pub(crate) fn is_sole_candidate(&self, address: &str) -> bool {
        matches!(self.candidate_addresses().as_slice(), [only] if *only == address)
    }

    pub(crate) fn focus_outcome(&self, focused: Option<&str>) -> FocusOutcome {
        match focused {
            Some(address) if self.client_address.as_deref() == Some(address) => {
                FocusOutcome::Primary
            }
            Some(_) => FocusOutcome::Fallback,
            None => FocusOutcome::NotFocused,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusOutcome {
    /// The primary window took focus: the event can be marked read.
    Primary,
    /// A sibling candidate took focus: the source window was not reached, so
    /// the event stays unread.
    Fallback,
    NotFocused,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct AgentEvent {
    pub(crate) id: String,
    pub(crate) agent: String,
    pub(crate) kind: String,
    #[serde(rename = "projectName")]
    pub(crate) project_name: String,
    #[serde(rename = "projectPath")]
    pub(crate) project_path: String,
    #[serde(
        default,
        rename = "projectKey",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) project_key: Option<String>,
    #[serde(
        default,
        rename = "branchName",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) branch_name: Option<String>,
    pub(crate) cwd: String,
    #[serde(rename = "sessionId")]
    pub(crate) session_id: String,
    #[serde(
        default,
        rename = "sessionTitle",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) session_title: Option<String>,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: String,
    pub(crate) workspace: Option<SourceWindow>,
    pub(crate) status: EventStatus,
    #[serde(flatten)]
    pub(crate) extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct AgentNotifierState {
    pub(crate) version: u8,
    pub(crate) events: Vec<AgentEvent>,
    #[serde(flatten)]
    pub(crate) extra: serde_json::Map<String, serde_json::Value>,
}

pub(crate) fn empty_state() -> AgentNotifierState {
    AgentNotifierState {
        version: 1,
        events: Vec::new(),
        extra: serde_json::Map::new(),
    }
}

pub(crate) fn parse_state(raw: &str) -> Result<AgentNotifierState, String> {
    let state: AgentNotifierState = serde_json::from_str(raw)
        .map_err(|error| format!("Invalid agent-notifier state: {error}"))?;
    if state.version != 1 {
        return Err("Invalid agent-notifier state".to_owned());
    }
    Ok(state)
}

pub(crate) fn append_and_trim(
    mut state: AgentNotifierState,
    event: AgentEvent,
) -> AgentNotifierState {
    state.events.insert(0, event);
    state.events = dedupe_events(state.events);
    state.events.truncate(STATE_LIMIT);
    state
}

pub(crate) fn set_event_status(
    mut state: AgentNotifierState,
    id: &str,
    status: EventStatus,
) -> AgentNotifierState {
    let target_key = state
        .events
        .iter()
        .find(|event| event.id == id)
        .and_then(dedupe_key);
    for event in &mut state.events {
        if event.id == id
            || target_key
                .as_ref()
                .is_some_and(|key| dedupe_key(event).as_ref() == Some(key))
        {
            event.status = status;
        }
    }
    state
}

pub(crate) fn clear_read_events(mut state: AgentNotifierState) -> AgentNotifierState {
    state
        .events
        .retain(|event| event.status != EventStatus::Read);
    state
}

pub(crate) fn set_window_address_read(
    mut state: AgentNotifierState,
    address: &str,
) -> AgentNotifierState {
    for event in &mut state.events {
        if event_matches_address(event, address) {
            event.status = EventStatus::Read;
        }
    }
    state
}

pub(crate) fn state_has_unread_for_address(state: &AgentNotifierState, address: &str) -> bool {
    state
        .events
        .iter()
        .any(|event| event.status == EventStatus::Unread && event_matches_address(event, address))
}

fn event_matches_address(event: &AgentEvent, address: &str) -> bool {
    event
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.client_address.as_deref())
        .is_some_and(|stored| stored == address)
}

fn dedupe_key(event: &AgentEvent) -> Option<String> {
    if !event.session_id.is_empty() && event.session_id != "unknown" {
        return Some(format!("{}:session:{}", event.agent, event.session_id));
    }
    let workspace = event.workspace.as_ref()?;
    Some(format!("{}:pid:{}", event.agent, workspace.client_pid))
}

pub(crate) fn dedupe_events(events: Vec<AgentEvent>) -> Vec<AgentEvent> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::with_capacity(events.len());
    for event in events {
        if let Some(key) = dedupe_key(&event) {
            if !seen.insert(key) {
                continue;
            }
        }
        deduped.push(event);
    }
    deduped
}

#[cfg(test)]
mod tests;
