use serde::Serialize;
use std::io;

use crate::app::Deps;
use crate::display::{
    build_info, display_state_from_events, status_output, unavailable_status_output, BuildInfo,
};
use crate::event::store::{read_state, with_state_update};
use crate::event::{focusable_events, mark_focused_window_events_read, AgentNotifierState};

pub(in crate::app) fn status_json(deps: &dyn Deps) -> io::Result<()> {
    let focused = deps.focused_window_address();
    let state = read_state_marking_focused_window_read(deps, focused.as_deref())?;
    let liveness = deps.liveness();
    let output =
        std::panic::catch_unwind(|| status_output(&focusable_events(&state.events, &liveness)))
            .unwrap_or_else(|_| unavailable_status_output());
    print_json(&output, deps)
}

pub(in crate::app) fn list_display_json(deps: &dyn Deps) -> io::Result<()> {
    let focused = deps.focused_window_address();
    let state = read_state_marking_focused_window_read(deps, focused.as_deref())?;
    let events = focusable_events(&state.events, &deps.liveness());
    print_json(&display_state_from_events(state.version, events), deps)
}

pub(in crate::app) fn list_json(deps: &dyn Deps) -> io::Result<()> {
    print_json(&read_state(&deps.state_path()?, deps.now())?, deps)
}

pub(in crate::app) fn version_json(deps: &dyn Deps) -> io::Result<()> {
    print_json(&build_metadata(deps), deps)
}

/// The read paths that mark: ADR 0001 makes reading the state the moment the
/// focused window's events become read, so the widget never writes read state.
fn read_state_marking_focused_window_read(
    deps: &dyn Deps,
    focused: Option<&str>,
) -> io::Result<AgentNotifierState> {
    with_state_update(&deps.state_path()?, deps.now(), |state| {
        mark_focused_window_events_read(state, focused)
    })
}

/// A serialization failure propagates: a shaped fallback here would lie to
/// every consumer except status-json, whose degradation lives in main.
fn print_json<T: Serialize>(value: &T, deps: &dyn Deps) -> io::Result<()> {
    let json = serde_json::to_string(value).map_err(io::Error::other)?;
    deps.print_line(&json);
    Ok(())
}

fn build_metadata(deps: &dyn Deps) -> BuildInfo {
    let state_path = deps.state_path().ok();
    build_info(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("AGENT_NOTIFIER_COMMIT"),
        env!("AGENT_NOTIFIER_DIRTY"),
        env!("AGENT_NOTIFIER_COMMIT_DATE"),
        state_path.as_deref(),
    )
}

#[cfg(test)]
mod tests;
