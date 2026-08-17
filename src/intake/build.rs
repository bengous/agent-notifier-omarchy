use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use std::ffi::OsStr;
use std::path::Path;

use crate::event::{AgentEvent, EventStatus, SourceWindow};
use crate::intake::agents::{Profile, SessionIdField};

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct HookInput {
    pub(crate) cwd: Option<String>,
    #[serde(default, alias = "sessionId")]
    pub(crate) session_id: Option<String>,
    #[serde(default, alias = "sessionFile")]
    pub(crate) session_file: Option<String>,
    #[serde(default, alias = "leafId")]
    pub(crate) leaf_id: Option<String>,
    #[serde(default)]
    pub(crate) transcript_path: Option<String>,
}

impl HookInput {
    fn session_id_value(&self, field: SessionIdField) -> Option<&str> {
        match field {
            SessionIdField::SessionId => self.session_id.as_deref(),
            SessionIdField::SessionFile => self.session_file.as_deref(),
            SessionIdField::LeafId => self.leaf_id.as_deref(),
        }
    }
}

pub(crate) fn parse_hook_input(raw: &str) -> HookInput {
    if raw.trim().is_empty() {
        return HookInput::default();
    }
    serde_json::from_str(raw).unwrap_or_default()
}

#[derive(Debug)]
pub(crate) struct CaptureContext {
    pub(crate) cwd: String,
    pub(crate) project_path: String,
    pub(crate) project_key: String,
    pub(crate) branch_name: Option<String>,
    pub(crate) workspace: Option<SourceWindow>,
    pub(crate) now: DateTime<Utc>,
    pub(crate) random_id: String,
    pub(crate) env_session_id: Option<String>,
}

pub(crate) fn build_event(
    profile: &Profile,
    input: &HookInput,
    context: CaptureContext,
) -> AgentEvent {
    let CaptureContext {
        cwd,
        project_path,
        project_key,
        branch_name,
        workspace,
        now,
        random_id,
        env_session_id,
    } = context;
    let project_name = Path::new(&project_path)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(&project_path)
        .to_owned();
    AgentEvent {
        id: format!("{}-{random_id}", now.timestamp_millis()),
        agent: profile.id.to_owned(),
        kind: "main".to_owned(),
        project_name,
        project_path,
        project_key: Some(project_key),
        branch_name,
        cwd,
        session_id: session_id(profile, input, env_session_id),
        session_title: None,
        created_at: now.to_rfc3339_opts(SecondsFormat::Millis, true),
        workspace,
        status: EventStatus::Unread,
        extra: serde_json::Map::new(),
    }
}

fn session_id(profile: &Profile, input: &HookInput, env_session_id: Option<String>) -> String {
    profile
        .session_id_fields
        .iter()
        .find_map(|field| input.session_id_value(*field))
        .map(str::to_owned)
        .or(env_session_id)
        .unwrap_or_else(|| "unknown".to_owned())
}

#[cfg(test)]
mod tests;
