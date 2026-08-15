use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use std::env;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::process::command_output;
use crate::state::{AgentEvent, EventStatus, WorkspaceInfo};

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct CodexStopInput {
    pub(crate) cwd: Option<String>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default, rename = "sessionId")]
    pub(crate) session_id_camel: Option<String>,
}

pub(crate) fn parse_codex_stop_input(raw: &str) -> CodexStopInput {
    if raw.trim().is_empty() {
        return CodexStopInput::default();
    }
    serde_json::from_str(raw).unwrap_or_default()
}

pub(crate) fn project_root(cwd: &str) -> String {
    command_output(["git", "-C", cwd, "rev-parse", "--show-toplevel"])
        .filter(|output| !output.is_empty())
        .unwrap_or_else(|| cwd.to_owned())
}

pub(crate) fn current_git_branch(cwd: &str) -> Option<String> {
    command_output(["git", "-C", cwd, "rev-parse", "--abbrev-ref", "HEAD"])
        .filter(|branch| !branch.is_empty() && branch != "HEAD")
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_stop_event(
    agent: &str,
    input: &CodexStopInput,
    cwd: String,
    project_path: String,
    branch_name: Option<String>,
    workspace: Option<WorkspaceInfo>,
    now: DateTime<Utc>,
    random_id: &str,
) -> AgentEvent {
    let project_name = Path::new(&project_path)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(&project_path)
        .to_owned();
    AgentEvent {
        id: format!("{}-{random_id}", now.timestamp_millis()),
        agent: agent.to_owned(),
        kind: "main".to_owned(),
        project_name,
        project_path,
        branch_name,
        cwd,
        session_id: input
            .session_id
            .clone()
            .or_else(|| input.session_id_camel.clone())
            .or_else(|| env::var("CODEX_SESSION_ID").ok())
            .unwrap_or_else(|| "unknown".to_owned()),
        created_at: now.to_rfc3339_opts(SecondsFormat::Millis, true),
        workspace,
        status: EventStatus::Unread,
    }
}

pub(crate) fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0_u8; bytes];
    if fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut buffer))
        .is_err()
    {
        let fallback = current_millis().to_le_bytes();
        for (index, byte) in buffer.iter_mut().enumerate() {
            *byte = fallback[index % fallback.len()];
        }
    }
    buffer
        .iter()
        .fold(String::with_capacity(bytes * 2), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        })
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}
