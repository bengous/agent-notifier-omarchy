use std::collections::HashSet;

use crate::event::model::{AgentEvent, AgentNotifierState, EventStatus, ProcessRef, SourceWindow};

const STATE_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureDecision {
    /// The user is already looking at the source window: nothing to notify.
    Discard,
    PersistAndAlert,
    /// Without an address the event could never be focused or expired, so it
    /// is not persisted. The alert still fires: losing the notification
    /// entirely is the one failure this tool exists to prevent. This path
    /// bypasses deduplication, because nothing is stored to deduplicate
    /// against.
    AlertOnly,
}

/// Discard a completion only when the source is certain: the candidate set is
/// exactly the focused window. An uncertain set keeps the event, even when the
/// best guess holds the focus. Of the rest, only events whose source window
/// can be addressed again are worth persisting.
pub(crate) fn capture_decision(event: &AgentEvent, focused: Option<&str>) -> CaptureDecision {
    let Some(source_window) = event.workspace.as_ref() else {
        return CaptureDecision::AlertOnly;
    };
    if focused.is_some_and(|address| source_window.is_sole_candidate(address)) {
        return CaptureDecision::Discard;
    }
    if source_window.client_address.is_some() {
        CaptureDecision::PersistAndAlert
    } else {
        CaptureDecision::AlertOnly
    }
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

/// What the compositor and /proc report right now, injected as data so every
/// lifecycle decision stays pure.
#[derive(Debug, Clone)]
pub(crate) struct SourceLiveness {
    pub(crate) existing_addresses: HashSet<String>,
    pub(crate) process_is_alive: fn(&ProcessRef) -> bool,
}

/// The source process is the per-window death signal: closing the window kills
/// its shell even when the terminal shares one pid across windows. Events
/// without one (legacy state) fall back to window-address liveness.
fn source_is_live(event: &AgentEvent, liveness: &SourceLiveness) -> bool {
    event
        .workspace
        .as_ref()
        .is_some_and(|workspace| match &workspace.source_process {
            Some(process) => (liveness.process_is_alive)(process),
            None => workspace
                .candidate_addresses()
                .iter()
                .any(|address| liveness.existing_addresses.contains(*address)),
        })
}

pub(crate) fn focusable_events(
    events: &[AgentEvent],
    liveness: &SourceLiveness,
) -> Vec<AgentEvent> {
    let focusable = events
        .iter()
        .filter(|event| source_is_live(event, liveness))
        .cloned()
        .collect::<Vec<_>>();
    dedupe_events(focusable)
}

impl SourceWindow {
    /// A certain source: the candidate set is exactly this one window.
    fn is_sole_candidate(&self, address: &str) -> bool {
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

/// The mark-read-on-focus contract: the binary alone decides read state — the
/// read queries mark the focused window's events read; the widget never writes
/// read state itself. The store's skip-write-when-unchanged keeps the widget's
/// file-watch refresh from looping on these read-path writes.
pub(crate) fn mark_focused_window_events_read(
    mut state: AgentNotifierState,
    focused: Option<&str>,
) -> AgentNotifierState {
    let Some(address) = focused else {
        return state;
    };
    for event in &mut state.events {
        if event_matches_address(event, address) {
            event.status = EventStatus::Read;
        }
    }
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

fn event_matches_address(event: &AgentEvent, address: &str) -> bool {
    event
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.client_address.as_deref())
        .is_some_and(|stored| stored == address)
}

pub(crate) fn clear_read_events(mut state: AgentNotifierState) -> AgentNotifierState {
    state
        .events
        .retain(|event| event.status != EventStatus::Read);
    state
}

pub(crate) fn prune_stale_events(
    mut state: AgentNotifierState,
    liveness: &SourceLiveness,
) -> AgentNotifierState {
    state.events.retain(|event| source_is_live(event, liveness));
    state
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
