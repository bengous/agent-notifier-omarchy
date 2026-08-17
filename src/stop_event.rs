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
use crate::state::{AgentEvent, EventStatus, SourceWindow};

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct StopHookInput {
    pub(crate) cwd: Option<String>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default, rename = "sessionId")]
    pub(crate) session_id_camel: Option<String>,
    #[serde(default)]
    pub(crate) transcript_path: Option<String>,
}

pub(crate) fn parse_stop_hook_input(raw: &str) -> StopHookInput {
    if raw.trim().is_empty() {
        return StopHookInput::default();
    }
    serde_json::from_str(raw).unwrap_or_default()
}

pub(crate) fn project_root(cwd: &str) -> String {
    command_output(["git", "-C", cwd, "rev-parse", "--show-toplevel"])
        .filter(|output| !output.is_empty())
        .unwrap_or_else(|| cwd.to_owned())
}

/// Every worktree of one repository shares the main repository as its key.
pub(crate) fn repository_key(cwd: &str, project_path: &str) -> String {
    command_output([
        "git",
        "-C",
        cwd,
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])
    .as_deref()
    .and_then(main_repository_root)
    .unwrap_or_else(|| project_path.to_owned())
}

fn main_repository_root(git_common_dir: &str) -> Option<String> {
    if git_common_dir.is_empty() {
        return None;
    }
    let path = Path::new(git_common_dir);
    if path.file_name() != Some(OsStr::new(".git")) {
        return Some(git_common_dir.to_owned());
    }
    path.parent()
        .map(|root| root.to_string_lossy().into_owned())
        .filter(|root| !root.is_empty())
}

pub(crate) fn current_git_branch(cwd: &str) -> Option<String> {
    command_output(["git", "-C", cwd, "rev-parse", "--abbrev-ref", "HEAD"])
        .filter(|branch| !branch.is_empty() && branch != "HEAD")
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_stop_event(
    agent: &str,
    input: &StopHookInput,
    cwd: String,
    project_path: String,
    project_key: String,
    branch_name: Option<String>,
    workspace: Option<SourceWindow>,
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
        project_key: Some(project_key),
        branch_name,
        cwd,
        session_id: input
            .session_id
            .clone()
            .or_else(|| input.session_id_camel.clone())
            .or_else(|| env::var("CODEX_SESSION_ID").ok())
            .unwrap_or_else(|| "unknown".to_owned()),
        session_title: None,
        created_at: now.to_rfc3339_opts(SecondsFormat::Millis, true),
        workspace,
        status: EventStatus::Unread,
        extra: serde_json::Map::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn a_worktree_common_dir_resolves_to_the_main_repository() {
        assert_eq!(
            main_repository_root("/repo/dotfiles/.git").as_deref(),
            Some("/repo/dotfiles")
        );
    }

    #[test]
    fn a_bare_repository_path_stays_the_key() {
        assert_eq!(
            main_repository_root("/repo/dotfiles.git").as_deref(),
            Some("/repo/dotfiles.git")
        );
    }

    #[test]
    fn a_git_directory_without_a_parent_yields_no_key() {
        assert_eq!(main_repository_root(""), None);
        assert_eq!(main_repository_root(".git"), None);
    }

    #[test]
    fn outside_a_repository_the_key_is_the_project_path() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempdir()?;

        assert_eq!(
            repository_key(&dir.path().to_string_lossy(), "/repo/dotfiles"),
            "/repo/dotfiles"
        );
        Ok(())
    }
}
