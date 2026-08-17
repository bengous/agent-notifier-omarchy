use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EventStatus {
    Unread,
    Read,
}

/// `start_time` (jiffies since boot, from `/proc/<pid>/stat`) makes the
/// reference immune to pid recycling.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessRef {
    pub(crate) pid: i64,
    pub(crate) start_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceWindow {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) monitor: String,
    pub(crate) client_pid: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) client_address: Option<String>,
    /// Every window that can be the source, best guess first. A single-process
    /// terminal gives all its windows one pid, so the true source window is not
    /// knowable at capture time. Invariant: the first entry is `client_address`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) client_addresses: Vec<String>,
    /// The window's own shell in the hook's process chain: the per-window
    /// liveness anchor a shared-pid terminal cannot provide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentEvent {
    pub(crate) id: String,
    pub(crate) agent: String,
    pub(crate) kind: String,
    pub(crate) project_name: String,
    pub(crate) project_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) project_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) branch_name: Option<String>,
    pub(crate) cwd: String,
    pub(crate) session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) session_title: Option<String>,
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

#[cfg(test)]
mod tests;
