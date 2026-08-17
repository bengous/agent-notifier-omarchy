use crate::event::model::{AgentEvent, AgentNotifierState, EventStatus, SourceWindow};

const STATE_LIMIT: usize = 50;

impl SourceWindow {
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
