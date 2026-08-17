use chrono::{DateTime, Utc};
use std::io::{self, Read};
use std::path::PathBuf;

use crate::alert;
use crate::event::store::state_path;
use crate::event::{AgentEvent, FocusOutcome, SourceLiveness, SourceWindow};
use crate::window::{hyprland, proc};

/// Everything `run` needs from the world, so that a command is dispatched the
/// same way in production and under test. Two adapters give this seam its
/// reason to exist: `SystemDeps` below, and the fake the end-to-end tests drive.
pub(crate) trait Deps {
    fn state_path(&self) -> io::Result<PathBuf>;
    fn now(&self) -> DateTime<Utc>;
    fn read_stdin(&self) -> io::Result<String>;
    fn print_line(&self, line: &str);
    fn focused_window_address(&self) -> Option<String>;
    fn current_source_window(&self) -> Option<SourceWindow>;
    fn liveness(&self) -> SourceLiveness;
    fn try_liveness(&self) -> io::Result<SourceLiveness>;
    fn focus_event_source(&self, event: Option<&AgentEvent>) -> FocusOutcome;
    fn watch_focused_window(&self, on_change: &mut dyn FnMut(&str)) -> io::Result<()>;
    fn alert(&self, app_name: &str, title: &str, body: &str);
}

#[derive(Debug)]
pub(crate) struct SystemDeps;

impl Deps for SystemDeps {
    fn state_path(&self) -> io::Result<PathBuf> {
        state_path()
    }

    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn read_stdin(&self) -> io::Result<String> {
        let mut raw = String::new();
        io::stdin().read_to_string(&mut raw)?;
        Ok(raw)
    }

    fn print_line(&self, line: &str) {
        println!("{line}");
    }

    fn focused_window_address(&self) -> Option<String> {
        hyprland::focused_window_address()
    }

    fn current_source_window(&self) -> Option<SourceWindow> {
        hyprland::current_source_window()
    }

    fn liveness(&self) -> SourceLiveness {
        SourceLiveness {
            existing_addresses: hyprland::existing_window_addresses(),
            process_is_alive: proc::process_is_alive,
        }
    }

    fn try_liveness(&self) -> io::Result<SourceLiveness> {
        Ok(SourceLiveness {
            existing_addresses: hyprland::try_existing_window_addresses()?,
            process_is_alive: proc::process_is_alive,
        })
    }

    fn focus_event_source(&self, event: Option<&AgentEvent>) -> FocusOutcome {
        hyprland::focus_event_source(event)
    }

    fn watch_focused_window(&self, on_change: &mut dyn FnMut(&str)) -> io::Result<()> {
        hyprland::watch_focused_window(on_change)
    }

    fn alert(&self, app_name: &str, title: &str, body: &str) {
        alert::alert(app_name, title, body);
    }
}
