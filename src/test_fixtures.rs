use chrono::{DateTime, Utc};
use std::ffi::OsStr;
use std::path::Path;

use crate::event::{empty_state, AgentEvent, AgentNotifierState, ProcessRef, SourceWindow};
use crate::intake::pi_event::{build_pi_event, PiHookInput};
use crate::intake::stop_event::{build_stop_event, StopHookInput};

pub(crate) fn fixture_clock() -> Result<DateTime<Utc>, Box<dyn std::error::Error>> {
    DateTime::from_timestamp_millis(1_778_061_600_000).ok_or_else(|| "invalid fixture clock".into())
}

pub(crate) fn base_event() -> AgentEvent {
    build_stop_event(
        "codex",
        &StopHookInput {
            cwd: Some("/repo/dotfiles".to_owned()),
            session_id: Some("session-1".to_owned()),
            session_id_camel: None,
            transcript_path: None,
        },
        "/repo/dotfiles".to_owned(),
        "/repo/dotfiles".to_owned(),
        "/repo/dotfiles".to_owned(),
        Some("main".to_owned()),
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
        DateTime::from_timestamp_millis(1_778_061_600_000).unwrap_or_else(Utc::now),
        "abcd",
    )
}

pub(crate) fn base_pi_event(workspace: Option<SourceWindow>) -> AgentEvent {
    build_pi_event(
        &PiHookInput {
            cwd: Some("/repo/dotfiles".to_owned()),
            session_id: None,
            session_id_camel: None,
            session_file: None,
            session_file_camel: Some("/repo/home/.pi/agent/sessions/pi-session.jsonl".to_owned()),
            leaf_id: None,
            leaf_id_camel: Some("leaf-1".to_owned()),
        },
        "/repo/dotfiles".to_owned(),
        "/repo/dotfiles".to_owned(),
        "/repo/dotfiles".to_owned(),
        Some("main".to_owned()),
        workspace,
        DateTime::from_timestamp_millis(1_778_061_600_000).unwrap_or_else(Utc::now),
        "bcde",
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
