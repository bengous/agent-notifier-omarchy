mod agent;
mod lifecycle;
mod model;
pub(crate) mod store;

pub(crate) use agent::Agent;
pub(crate) use lifecycle::{
    append_and_trim, clear_read_events, dedupe_events, set_event_status, set_window_address_read,
    state_has_unread_for_address, FocusOutcome,
};
pub(crate) use model::{
    empty_state, parse_state, AgentEvent, AgentNotifierState, EventStatus, ProcessRef, SourceWindow,
};
