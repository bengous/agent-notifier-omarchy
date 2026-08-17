pub(crate) mod cli;
mod deps;

use serde::Serialize;
use std::io;

use crate::app::cli::CliCommand;
use crate::display::{
    build_info, display_state_from_events, event_label, status_output, unavailable_status_output,
    BuildInfo,
};
use crate::event::store::{read_state, with_state_update};
use crate::event::{
    append_and_trim, capture_decision, clear_read_events, empty_state,
    mark_focused_window_events_read, prune_stale_events, set_event_status, Agent, AgentEvent,
    AgentNotifierState, CaptureDecision, EventStatus, FocusOutcome,
};
use crate::intake;

pub(crate) use crate::display::UNAVAILABLE_STATUS_JSON;
pub(crate) use deps::{Deps, SystemDeps};

fn mark_address_read(address: &str, deps: &dyn Deps) -> io::Result<()> {
    let _ = with_state_update(&deps.state_path()?, deps.now(), |state| {
        mark_focused_window_events_read(state, Some(address))
    })?;
    Ok(())
}

fn focusable_events(events: &[AgentEvent], deps: &dyn Deps) -> Vec<AgentEvent> {
    crate::event::focusable_events(events, &deps.liveness())
}

fn read_state_with_focused_window_read(
    deps: &dyn Deps,
    focused: Option<&str>,
) -> io::Result<AgentNotifierState> {
    with_state_update(&deps.state_path()?, deps.now(), |state| {
        mark_focused_window_events_read(state, focused)
    })
}

fn capture_completion_event(agent: Agent, event: &AgentEvent, deps: &dyn Deps) -> io::Result<()> {
    match capture_decision(event, deps.focused_window_address().as_deref()) {
        CaptureDecision::Discard => return Ok(()),
        CaptureDecision::PersistAndAlert => {
            with_state_update(&deps.state_path()?, deps.now(), |state| {
                append_and_trim(state, event.clone())
            })?;
        }
        CaptureDecision::AlertOnly => {
            eprintln!(
                "agent-notifier: no Hyprland client address for this completion; alerting without storing"
            );
        }
    }
    let agent_name = agent.display_name();
    deps.alert(
        agent_name,
        &format!("{agent_name} completed"),
        &event_label(event),
    );
    Ok(())
}

fn handle_agent_hook(agent: Agent, deps: &dyn Deps) -> io::Result<()> {
    let raw = deps.read_stdin()?;
    let event = intake::capture(agent, &raw, deps.current_source_window(), deps.now());
    capture_completion_event(agent, &event, deps)
}

fn handle_status_json(deps: &dyn Deps) -> io::Result<()> {
    let focused = deps.focused_window_address();
    let state = read_state_with_focused_window_read(deps, focused.as_deref())?;
    let liveness = deps.liveness();
    let output = std::panic::catch_unwind(|| {
        status_output(&crate::event::focusable_events(&state.events, &liveness))
    })
    .unwrap_or_else(|_| unavailable_status_output());
    print_json(&output, deps)
}

fn handle_focused_window_read(deps: &dyn Deps) -> io::Result<()> {
    let Some(address) = deps.focused_window_address() else {
        return Ok(());
    };
    mark_address_read(&address, deps)
}

fn handle_watch_focused_window(deps: &dyn Deps) -> io::Result<()> {
    deps.watch_focused_window(&mut |address| {
        if let Err(error) = mark_address_read(address, deps) {
            eprintln!("agent-notifier: state update failed: {error}");
        }
    })
}

fn focus_event(event: Option<&AgentEvent>, target: &str, deps: &dyn Deps) -> io::Result<i32> {
    match deps.focus_event_source(event) {
        FocusOutcome::NotFocused => {
            eprintln!("agent-notifier: could not focus the source window for {target}");
            Ok(1)
        }
        FocusOutcome::Primary => {
            if let Some(id) = event.map(|event| event.id.clone()) {
                let _ = with_state_update(&deps.state_path()?, deps.now(), |state| {
                    set_event_status(state, &id, EventStatus::Read)
                })?;
            }
            Ok(0)
        }
        FocusOutcome::Fallback => Ok(0),
    }
}

/// A serialization failure propagates: a shaped fallback here would lie to
/// every consumer except status-json, whose degradation lives in main.
fn print_json<T: Serialize>(value: &T, deps: &dyn Deps) -> io::Result<()> {
    let json = serde_json::to_string(value).map_err(io::Error::other)?;
    deps.print_line(&json);
    Ok(())
}

fn crate_build_info(deps: &dyn Deps) -> BuildInfo {
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

fn usage() -> &'static str {
    "Usage: agent-notifier <command>

Commands:
  hook                     Capture a Codex completion from stdin
  pi-hook                  Capture a Pi completion from stdin
  claude-hook              Capture a Claude Code completion from stdin
  status-json              Print bar-widget status JSON
  list-display-json        Print focusable events as display JSON
  version-json             Print build metadata as JSON
  focus-id <event-id>      Focus an event by id
  mark-read <event-id>     Mark an event as read
  watch-focused-window     Watch focused-window changes
  clear-read               Remove read events
  clear-all                Remove all events
  prune-stale              Remove events whose source window is gone

Options:
  -h, --help               Print help
  -V, --version            Print version"
}

pub(crate) fn run(command: &CliCommand, deps: &dyn Deps) -> io::Result<i32> {
    match command {
        CliCommand::Help => {
            deps.print_line(usage());
            Ok(0)
        }
        CliCommand::Version => {
            deps.print_line(&format!("agent-notifier {}", env!("CARGO_PKG_VERSION")));
            Ok(0)
        }
        CliCommand::Hook => handle_agent_hook(Agent::Codex, deps).map(|()| 0),
        CliCommand::PiHook => handle_agent_hook(Agent::Pi, deps).map(|()| 0),
        CliCommand::ClaudeHook => handle_agent_hook(Agent::Claude, deps).map(|()| 0),
        CliCommand::StatusJson => handle_status_json(deps).map(|()| 0),
        // TODO(contract): no known consumer — retire or test before v1.
        CliCommand::ListJson => {
            print_json(&read_state(&deps.state_path()?, deps.now())?, deps)?;
            Ok(0)
        }
        CliCommand::ListDisplayJson => {
            let focused = deps.focused_window_address();
            let state = read_state_with_focused_window_read(deps, focused.as_deref())?;
            print_json(
                &display_state_from_events(state.version, focusable_events(&state.events, deps)),
                deps,
            )?;
            Ok(0)
        }
        CliCommand::VersionJson => {
            print_json(&crate_build_info(deps), deps)?;
            Ok(0)
        }
        // TODO(contract): no known consumer — retire or test before v1.
        CliCommand::FocusLatest => {
            let state = read_state(&deps.state_path()?, deps.now())?;
            let focusable = focusable_events(&state.events, deps);
            let event = focusable
                .iter()
                .find(|event| event.status == EventStatus::Unread);
            focus_event(event, "the latest unread event", deps)
        }
        CliCommand::FocusId(id) => {
            let state = read_state(&deps.state_path()?, deps.now())?;
            let event = state.events.iter().find(|event| event.id == *id);
            focus_event(event, id, deps)
        }
        CliCommand::MarkRead(id) => {
            let _ = with_state_update(&deps.state_path()?, deps.now(), |state| {
                set_event_status(state, id, EventStatus::Read)
            })?;
            Ok(0)
        }
        CliCommand::FocusIdMissing => {
            eprintln!("agent-notifier: focus-id requires an event id");
            Ok(2)
        }
        CliCommand::MarkReadMissing => {
            eprintln!("agent-notifier: mark-read requires an event id");
            Ok(2)
        }
        // TODO(contract): no known consumer — retire or test before v1.
        CliCommand::FocusedWindowRead => handle_focused_window_read(deps).map(|()| 0),
        CliCommand::WatchFocusedWindow => handle_watch_focused_window(deps).map(|()| 0),
        CliCommand::ClearRead => {
            let _ = with_state_update(&deps.state_path()?, deps.now(), clear_read_events)?;
            Ok(0)
        }
        CliCommand::ClearAll => {
            let _ = with_state_update(&deps.state_path()?, deps.now(), |_| empty_state())?;
            Ok(0)
        }
        CliCommand::PruneStale => {
            let liveness = deps.try_liveness()?;
            let _ = with_state_update(&deps.state_path()?, deps.now(), |state| {
                prune_stale_events(state, &liveness)
            })?;
            Ok(0)
        }
        CliCommand::Unknown => {
            eprintln!("{}", usage());
            Ok(2)
        }
    }
}

#[cfg(test)]
mod tests;
