use chrono::{DateTime, Utc};
use std::cell::RefCell;
use std::collections::HashSet;
use std::error::Error;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::app::Deps;
use crate::event::{
    empty_state, Agent, AgentEvent, AgentNotifierState, FocusOutcome, ProcessRef, SourceLiveness,
    SourceWindow,
};
use crate::intake::agents::profile;
use crate::intake::build::{build_event, CaptureContext, HookInput};
use crate::setup::{HarnessReport, HarnessState, SetupReport};

pub(crate) fn fixture_clock() -> DateTime<Utc> {
    DateTime::UNIX_EPOCH + Duration::from_secs(1_778_061_600)
}

/// One event in the frozen `events.json` v1 shape, spelled key by key: a field
/// added to the Rust types must not silently enter the v1 fixture.
pub(crate) fn v1_state_json() -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "events": [{
            "id": "e",
            "agent": "claude",
            "kind": "main",
            "projectName": "p",
            "projectPath": "/repo/dotfiles",
            "cwd": "/repo/dotfiles",
            "sessionId": "s",
            "createdAt": "2026-07-26T08:00:00.000Z",
            "workspace": {
                "id": 1,
                "name": "1",
                "monitor": "DP-3",
                "clientPid": 42,
                "title": "t"
            },
            "status": "unread"
        }]
    })
}

fn no_process_is_alive(_process: &ProcessRef) -> bool {
    false
}

pub(crate) fn probe_row(
    agent: Agent,
    state: HarnessState,
    hook_command: Option<&str>,
) -> HarnessReport {
    let config_path = match agent {
        Agent::Claude => "/repo/home/.claude/settings.json",
        Agent::Codex => "/repo/home/.codex/config.toml",
        Agent::Pi => "/repo/home/.pi/agent/extensions/agent-notifier.ts",
    };
    HarnessReport {
        harness: agent.id().to_owned(),
        display_name: agent.display_name().to_owned(),
        state,
        config_path: config_path.to_owned(),
        hook_command: hook_command.map(str::to_owned),
    }
}

pub(crate) fn nothing_installed_probe() -> SetupReport {
    SetupReport {
        version: 1,
        binary_on_path: false,
        listener_live: false,
        harnesses: vec![
            probe_row(Agent::Claude, HarnessState::HarnessAbsent, None),
            probe_row(Agent::Codex, HarnessState::HarnessAbsent, None),
            probe_row(Agent::Pi, HarnessState::HarnessAbsent, None),
        ],
    }
}

pub(crate) fn wired_probe() -> SetupReport {
    SetupReport {
        version: 1,
        binary_on_path: true,
        listener_live: true,
        harnesses: vec![
            probe_row(
                Agent::Claude,
                HarnessState::Wired,
                Some("agent-notifier claude-hook"),
            ),
            probe_row(
                Agent::Codex,
                HarnessState::Wired,
                Some("agent-notifier hook"),
            ),
            probe_row(
                Agent::Pi,
                HarnessState::Wired,
                Some("agent-notifier pi-hook"),
            ),
        ],
    }
}

/// The second adapter of the `Deps` seam: a world a test writes by hand, so
/// `run` can be driven end to end without a compositor, a clock or a terminal.
#[derive(Debug)]
pub(crate) struct FakeDeps {
    pub(crate) state_path: PathBuf,
    pub(crate) now: DateTime<Utc>,
    pub(crate) stdin: String,
    pub(crate) focused_window_address: Option<String>,
    pub(crate) existing_window_addresses: HashSet<String>,
    pub(crate) process_is_alive: fn(&ProcessRef) -> bool,
    pub(crate) source_window: Option<SourceWindow>,
    pub(crate) focus_outcome: FocusOutcome,
    pub(crate) focused_window_changes: Vec<String>,
    pub(crate) printed_lines: RefCell<Vec<String>>,
    pub(crate) alerts: RefCell<Vec<[String; 3]>>,
    pub(crate) setup_probe: SetupReport,
}

impl FakeDeps {
    pub(crate) fn new(state_path: PathBuf) -> Self {
        Self {
            state_path,
            now: fixture_clock(),
            stdin: String::new(),
            focused_window_address: None,
            existing_window_addresses: HashSet::new(),
            process_is_alive: no_process_is_alive,
            source_window: None,
            focus_outcome: FocusOutcome::NotFocused,
            focused_window_changes: Vec::new(),
            printed_lines: RefCell::new(Vec::new()),
            alerts: RefCell::new(Vec::new()),
            setup_probe: nothing_installed_probe(),
        }
    }

    pub(crate) fn printed(&self) -> String {
        self.printed_lines.borrow().join("\n")
    }

    pub(crate) fn printed_json(&self) -> Result<serde_json::Value, Box<dyn Error>> {
        let lines = self.printed_lines.borrow();
        let [line] = lines.as_slice() else {
            return Err(format!("expected exactly one printed line, got {}", lines.len()).into());
        };
        Ok(serde_json::from_str(line)?)
    }

    pub(crate) fn stored_state(&self) -> Result<AgentNotifierState, Box<dyn Error>> {
        Ok(crate::event::parse_state(&std::fs::read_to_string(
            &self.state_path,
        )?)?)
    }

    pub(crate) fn stored_event(&self, id: &str) -> Result<AgentEvent, Box<dyn Error>> {
        self.stored_state()?
            .events
            .into_iter()
            .find(|event| event.id == id)
            .ok_or_else(|| format!("no stored event {id}").into())
    }
}

impl Deps for FakeDeps {
    fn state_path(&self) -> io::Result<PathBuf> {
        Ok(self.state_path.clone())
    }

    fn now(&self) -> DateTime<Utc> {
        self.now
    }

    fn read_stdin(&self) -> io::Result<String> {
        Ok(self.stdin.clone())
    }

    fn print_line(&self, line: &str) {
        self.printed_lines.borrow_mut().push(line.to_owned());
    }

    fn focused_window_address(&self) -> Option<String> {
        self.focused_window_address.clone()
    }

    fn current_source_window(&self) -> Option<SourceWindow> {
        self.source_window.clone()
    }

    fn liveness(&self) -> SourceLiveness {
        SourceLiveness {
            existing_addresses: self.existing_window_addresses.clone(),
            process_is_alive: self.process_is_alive,
        }
    }

    fn try_liveness(&self) -> io::Result<SourceLiveness> {
        Ok(self.liveness())
    }

    fn focus_event_source(&self, event: Option<&AgentEvent>) -> FocusOutcome {
        event.map_or(FocusOutcome::NotFocused, |_| self.focus_outcome)
    }

    fn watch_focused_window(&self, on_change: &mut dyn FnMut(&str)) -> io::Result<()> {
        for address in &self.focused_window_changes {
            on_change(address);
        }
        Ok(())
    }

    fn alert(&self, app_name: &str, title: &str, body: &str) {
        self.alerts
            .borrow_mut()
            .push([app_name.to_owned(), title.to_owned(), body.to_owned()]);
    }

    fn setup_probe(&self) -> SetupReport {
        self.setup_probe.clone()
    }
}

fn fixture_context(workspace: Option<SourceWindow>, random_id: &str) -> CaptureContext {
    CaptureContext {
        cwd: "/repo/dotfiles".to_owned(),
        project_path: "/repo/dotfiles".to_owned(),
        project_key: "/repo/dotfiles".to_owned(),
        branch_name: Some("main".to_owned()),
        workspace,
        now: fixture_clock(),
        random_id: random_id.to_owned(),
        env_session_id: None,
    }
}

pub(crate) fn base_event() -> AgentEvent {
    build_event(
        profile(Agent::Codex),
        &HookInput {
            cwd: Some("/repo/dotfiles".to_owned()),
            session_id: Some("session-1".to_owned()),
            ..HookInput::default()
        },
        fixture_context(
            Some(SourceWindow {
                id: 3,
                name: "3".to_owned(),
                monitor: "DP-3".to_owned(),
                client_pid: 300,
                client_address: None,
                client_addresses: Vec::new(),
                source_process: None,
                title: "dotfiles | main".to_owned(),
                extra: serde_json::Map::new(),
            }),
            "abcd",
        ),
    )
}

pub(crate) fn base_pi_event(workspace: Option<SourceWindow>) -> AgentEvent {
    build_event(
        profile(Agent::Pi),
        &HookInput {
            cwd: Some("/repo/dotfiles".to_owned()),
            session_file: Some("/repo/home/.pi/agent/sessions/pi-session.jsonl".to_owned()),
            leaf_id: Some("leaf-1".to_owned()),
            ..HookInput::default()
        },
        fixture_context(workspace, "bcde"),
    )
}

pub(crate) fn event_with_session(id: &str, session_id: &str) -> AgentEvent {
    AgentEvent {
        id: id.to_owned(),
        session_id: session_id.to_owned(),
        ..base_event()
    }
}

pub(crate) fn sessionless_event(id: &str, session_id: &str) -> AgentEvent {
    AgentEvent {
        workspace: None,
        ..event_with_session(id, session_id)
    }
}

pub(crate) fn event_with_pid(id: &str, pid: i64) -> AgentEvent {
    let mut base = base_event();
    if let Some(workspace) = &mut base.workspace {
        workspace.client_pid = pid;
    }
    AgentEvent {
        id: id.to_owned(),
        session_id: format!("session-{pid}"),
        ..base
    }
}

pub(crate) fn event_with_address(id: &str, pid: i64, address: &str) -> AgentEvent {
    let mut base = event_with_pid(id, pid);
    if let Some(workspace) = &mut base.workspace {
        workspace.client_address = Some(address.to_owned());
    }
    base
}

pub(crate) fn event_with_candidates(id: &str, pid: i64, addresses: &[&str]) -> AgentEvent {
    let mut base = event_with_pid(id, pid);
    if let Some(workspace) = &mut base.workspace {
        workspace.client_address = addresses.first().map(|address| (*address).to_owned());
        workspace.client_addresses = addresses
            .iter()
            .map(|address| (*address).to_owned())
            .collect();
    }
    base
}

pub(crate) fn event_with_source_process(id: &str, process: ProcessRef) -> AgentEvent {
    let mut base = event_with_address(id, 4682, "0xguess");
    if let Some(workspace) = &mut base.workspace {
        workspace.source_process = Some(process);
    }
    base
}

pub(crate) fn event_in_project(
    id: &str,
    pid: i64,
    project_key: Option<&str>,
    project_path: &str,
) -> AgentEvent {
    AgentEvent {
        project_key: project_key.map(str::to_owned),
        project_name: Path::new(project_path)
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or(project_path)
            .to_owned(),
        project_path: project_path.to_owned(),
        ..event_with_pid(id, pid)
    }
}

pub(crate) fn workspace(event: &AgentEvent) -> Result<SourceWindow, Box<dyn std::error::Error>> {
    event
        .workspace
        .clone()
        .ok_or_else(|| "missing workspace".into())
}

pub(crate) fn state_of(events: Vec<AgentEvent>) -> AgentNotifierState {
    AgentNotifierState {
        events,
        ..empty_state()
    }
}
