pub(crate) mod agents;
pub(crate) mod build;
mod git;
mod id;
mod session_title;

use chrono::{DateTime, Utc};
use std::env;
use std::path::Path;

use crate::event::{Agent, AgentEvent, SourceWindow};
use crate::intake::agents::TitleSource;
use crate::intake::build::{build_event, parse_hook_input, CaptureContext, HookInput};

pub(crate) fn capture(
    agent: Agent,
    raw_stdin: &str,
    workspace: Option<SourceWindow>,
    now: DateTime<Utc>,
) -> AgentEvent {
    let profile = agents::profile(agent);
    let input = parse_hook_input(raw_stdin);
    let cwd = input.cwd.clone().unwrap_or_else(fallback_cwd);
    let project_path = git::project_root(&cwd);
    let project_key = git::repository_key(&cwd, &project_path);
    let branch_name = git::current_git_branch(&project_path);
    let event = build_event(
        profile,
        &input,
        CaptureContext {
            cwd,
            project_path,
            project_key,
            branch_name,
            workspace,
            now,
            random_id: id::random_hex(4),
            env_session_id: profile
                .session_id_env_var
                .and_then(|var| env::var(var).ok()),
        },
    );
    AgentEvent {
        session_title: resolve_session_title(profile.title_source, &input, &event),
        ..event
    }
}

fn fallback_cwd() -> String {
    env::var("PWD")
        .ok()
        .or_else(|| {
            env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| ".".to_owned())
}

fn hook_session_id(event: &AgentEvent) -> Option<&str> {
    Some(event.session_id.as_str()).filter(|id| !id.is_empty() && *id != "unknown")
}

fn resolve_session_title(
    source: Option<TitleSource>,
    input: &HookInput,
    event: &AgentEvent,
) -> Option<String> {
    let session_id = hook_session_id(event);
    match source? {
        TitleSource::ClaudeTranscript => {
            let transcript_path = input.transcript_path.as_deref()?;
            session_title::claude_session_title(Path::new(transcript_path), session_id)
        }
        TitleSource::CodexSessions => {
            session_title::codex_session_title(&session_title::codex_sessions_dir()?, session_id?)
        }
    }
}
