use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const TITLE_LIMIT: usize = 200;

#[derive(Debug, Deserialize)]
struct TranscriptLine {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default, rename = "aiTitle")]
    ai_title: Option<String>,
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
    #[serde(default)]
    message: Option<TranscriptMessage>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessage {
    #[serde(default)]
    content: Option<MessageContent>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RolloutLine {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    payload: Option<RolloutPayload>,
}

#[derive(Debug, Deserialize)]
struct RolloutPayload {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

pub(crate) fn claude_session_title(
    transcript_path: &Path,
    session_id: Option<&str>,
) -> Option<String> {
    let raw = fs::read_to_string(transcript_path).ok()?;
    transcript_title(&raw, session_id)
}

pub(crate) fn codex_session_title(sessions_dir: &Path, session_id: &str) -> Option<String> {
    let rollout = find_rollout(sessions_dir, session_id)?;
    let raw = fs::read_to_string(rollout).ok()?;
    rollout_title(&raw)
}

pub(crate) fn codex_sessions_dir() -> Option<PathBuf> {
    let home = env::var_os("CODEX_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|home| !home.is_empty())
                .map(|home| PathBuf::from(home).join(".codex"))
        })?;
    Some(home.join("sessions"))
}

fn transcript_title(raw: &str, session_id: Option<&str>) -> Option<String> {
    last_ai_title(raw, session_id).or_else(|| first_user_title(raw))
}

fn last_ai_title(raw: &str, session_id: Option<&str>) -> Option<String> {
    raw.lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<TranscriptLine>(line).ok())
        .filter(|line| line.kind.as_deref() == Some("ai-title"))
        .filter(|line| session_id.is_none_or(|id| line.session_id.as_deref() == Some(id)))
        .find_map(|line| normalize_title(line.ai_title.as_deref()?))
}

fn first_user_title(raw: &str) -> Option<String> {
    raw.lines()
        .filter_map(|line| serde_json::from_str::<TranscriptLine>(line).ok())
        .filter(|line| line.kind.as_deref() == Some("user"))
        .find_map(|line| message_title(line.message?.content?))
}

fn message_title(content: MessageContent) -> Option<String> {
    match content {
        MessageContent::Text(text) => usable_title(&text),
        MessageContent::Blocks(blocks) => blocks
            .into_iter()
            .filter(|block| block.kind.as_deref() == Some("text"))
            .find_map(|block| usable_title(block.text.as_deref()?)),
    }
}

fn usable_title(text: &str) -> Option<String> {
    // Claude Code wraps commands and meta prompts in XML-like tags the user never typed.
    normalize_title(text).filter(|title| !title.starts_with('<'))
}

fn rollout_title(raw: &str) -> Option<String> {
    raw.lines()
        .filter_map(|line| serde_json::from_str::<RolloutLine>(line).ok())
        .filter(|line| line.kind.as_deref() == Some("event_msg"))
        .find_map(|line| {
            let payload = line.payload?;
            if payload.kind.as_deref() != Some("user_message") {
                return None;
            }
            usable_title(&payload.message?)
        })
}

fn find_rollout(sessions_dir: &Path, session_id: &str) -> Option<PathBuf> {
    let suffix = format!("-{session_id}.jsonl");
    let mut pending = vec![sessions_dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if entry.file_name().to_string_lossy().ends_with(&suffix) {
                return Some(entry.path());
            }
        }
    }
    None
}

fn normalize_title(raw: &str) -> Option<String> {
    let line = raw.trim().lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    Some(line.chars().take(TITLE_LIMIT).collect())
}

#[cfg(test)]
mod tests;
