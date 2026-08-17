use std::io;

use crate::app::Deps;
use crate::event::store::{read_state, with_state_update};
use crate::event::{
    clear_read_events, empty_state, focusable_events, mark_focused_window_events_read,
    prune_stale_events, set_event_status, AgentEvent, EventStatus, FocusOutcome,
};

pub(in crate::app) fn focus_id(id: &str, deps: &dyn Deps) -> io::Result<i32> {
    let state = read_state(&deps.state_path()?, deps.now())?;
    let event = state.events.iter().find(|event| event.id == id);
    focus_event(event, id, deps)
}

pub(in crate::app) fn focus_latest(deps: &dyn Deps) -> io::Result<i32> {
    let state = read_state(&deps.state_path()?, deps.now())?;
    let focusable = focusable_events(&state.events, &deps.liveness());
    let event = focusable
        .iter()
        .find(|event| event.status == EventStatus::Unread);
    focus_event(event, "the latest unread event", deps)
}

pub(in crate::app) fn mark_read(id: &str, deps: &dyn Deps) -> io::Result<()> {
    let _ = with_state_update(&deps.state_path()?, deps.now(), |state| {
        set_event_status(state, id, EventStatus::Read)
    })?;
    Ok(())
}

pub(in crate::app) fn focused_window_read(deps: &dyn Deps) -> io::Result<()> {
    let Some(address) = deps.focused_window_address() else {
        return Ok(());
    };
    mark_address_read(&address, deps)
}

pub(in crate::app) fn mark_address_read(address: &str, deps: &dyn Deps) -> io::Result<()> {
    let _ = with_state_update(&deps.state_path()?, deps.now(), |state| {
        mark_focused_window_events_read(state, Some(address))
    })?;
    Ok(())
}

pub(in crate::app) fn clear_read(deps: &dyn Deps) -> io::Result<()> {
    let _ = with_state_update(&deps.state_path()?, deps.now(), clear_read_events)?;
    Ok(())
}

pub(in crate::app) fn clear_all(deps: &dyn Deps) -> io::Result<()> {
    let _ = with_state_update(&deps.state_path()?, deps.now(), |_| empty_state())?;
    Ok(())
}

pub(in crate::app) fn prune_stale(deps: &dyn Deps) -> io::Result<()> {
    let liveness = deps.try_liveness()?;
    let _ = with_state_update(&deps.state_path()?, deps.now(), |state| {
        prune_stale_events(state, &liveness)
    })?;
    Ok(())
}

fn focus_event(event: Option<&AgentEvent>, target: &str, deps: &dyn Deps) -> io::Result<i32> {
    match deps.focus_event_source(event) {
        FocusOutcome::NotFocused => {
            eprintln!("agent-notifier: could not focus the source window for {target}");
            Ok(1)
        }
        FocusOutcome::Primary => {
            if let Some(id) = event.map(|event| event.id.clone()) {
                mark_read(&id, deps)?;
            }
            Ok(0)
        }
        FocusOutcome::Fallback => Ok(0),
    }
}
