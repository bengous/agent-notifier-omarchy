use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use std::env;
use std::ffi::OsStr;
use std::path::Path;

use crate::state::{AgentEvent, EventStatus, WorkspaceInfo};

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct PiHookInput {
    pub(crate) cwd: Option<String>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default, rename = "sessionId")]
    pub(crate) session_id_camel: Option<String>,
    #[serde(default)]
    pub(crate) session_file: Option<String>,
    #[serde(default, rename = "sessionFile")]
    pub(crate) session_file_camel: Option<String>,
    #[serde(default)]
    pub(crate) leaf_id: Option<String>,
    #[serde(default, rename = "leafId")]
    pub(crate) leaf_id_camel: Option<String>,
}

pub(crate) fn parse_pi_hook_input(raw: &str) -> PiHookInput {
    if raw.trim().is_empty() {
        return PiHookInput::default();
    }
    serde_json::from_str(raw).unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_pi_event(
    input: &PiHookInput,
    cwd: String,
    project_path: String,
    project_key: String,
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
        agent: "pi".to_owned(),
        kind: "main".to_owned(),
        project_name,
        project_path,
        project_key: Some(project_key),
        branch_name,
        cwd,
        session_id: input
            .session_id
            .clone()
            .or_else(|| input.session_id_camel.clone())
            .or_else(|| input.session_file.clone())
            .or_else(|| input.session_file_camel.clone())
            .or_else(|| input.leaf_id.clone())
            .or_else(|| input.leaf_id_camel.clone())
            .or_else(|| env::var("PI_SESSION_ID").ok())
            .unwrap_or_else(|| "unknown".to_owned()),
        session_title: None,
        created_at: now.to_rfc3339_opts(SecondsFormat::Millis, true),
        workspace,
        status: EventStatus::Unread,
    }
}
