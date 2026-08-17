mod capture;
pub(crate) mod cli;
mod deps;
mod focus;
mod query;
mod watch;

use std::io;

use crate::app::cli::CliCommand;
use crate::event::Agent;

pub(crate) use crate::display::UNAVAILABLE_STATUS_JSON;
pub(crate) use deps::{Deps, SystemDeps};

fn usage() -> &'static str {
    "Usage: agent-notifier <command>

Commands:
  hook                     Capture a Codex completion from stdin
  pi-hook                  Capture a Pi completion from stdin
  claude-hook              Capture a Claude Code completion from stdin
  status-json              Print bar-widget status JSON
  list-display-json        Print focusable events as display JSON
  version-json             Print build metadata as JSON
  doctor [--json]          Diagnose the harness wiring
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
        CliCommand::Hook => capture::hook(Agent::Codex, deps).map(|()| 0),
        CliCommand::PiHook => capture::hook(Agent::Pi, deps).map(|()| 0),
        CliCommand::ClaudeHook => capture::hook(Agent::Claude, deps).map(|()| 0),
        CliCommand::StatusJson => query::status_json(deps).map(|()| 0),
        // TODO(contract): no known consumer — retire or test before v1.
        CliCommand::ListJson => query::list_json(deps).map(|()| 0),
        CliCommand::ListDisplayJson => query::list_display_json(deps).map(|()| 0),
        CliCommand::VersionJson => query::version_json(deps).map(|()| 0),
        CliCommand::Doctor => {
            query::doctor(deps);
            Ok(0)
        }
        CliCommand::DoctorJson => query::doctor_json(deps).map(|()| 0),
        // TODO(contract): no known consumer — retire or test before v1.
        CliCommand::FocusLatest => focus::focus_latest(deps),
        CliCommand::FocusId(id) => focus::focus_id(id, deps),
        CliCommand::MarkRead(id) => focus::mark_read(id, deps).map(|()| 0),
        CliCommand::FocusIdMissing => {
            eprintln!("agent-notifier: focus-id requires an event id");
            Ok(2)
        }
        CliCommand::MarkReadMissing => {
            eprintln!("agent-notifier: mark-read requires an event id");
            Ok(2)
        }
        // TODO(contract): no known consumer — retire or test before v1.
        CliCommand::FocusedWindowRead => focus::focused_window_read(deps).map(|()| 0),
        CliCommand::WatchFocusedWindow => watch::focused_window(deps).map(|()| 0),
        CliCommand::ClearRead => focus::clear_read(deps).map(|()| 0),
        CliCommand::ClearAll => focus::clear_all(deps).map(|()| 0),
        CliCommand::PruneStale => focus::prune_stale(deps).map(|()| 0),
        CliCommand::Unknown => {
            eprintln!("{}", usage());
            Ok(2)
        }
    }
}

#[cfg(test)]
mod tests;
