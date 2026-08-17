mod agent;
mod lifecycle;
mod model;
pub(crate) mod store;

pub(crate) use agent::Agent;
pub(crate) use lifecycle::{
    append_and_trim, capture_decision, clear_read_events, dedupe_events, focusable_events,
    mark_focused_window_events_read, prune_stale_events, set_event_status, CaptureDecision,
    FocusOutcome, SourceLiveness,
};
pub(crate) use model::{
    empty_state, parse_state, AgentEvent, AgentNotifierState, EventStatus, ProcessRef, SourceWindow,
};
