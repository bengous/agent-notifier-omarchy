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
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    const SESSION: &str = "5861a287-0000-0000-0000-000000000001";

    fn ai_title_line(title: &str, session_id: &str) -> String {
        format!(r#"{{"type":"ai-title","aiTitle":"{title}","sessionId":"{session_id}"}}"#)
    }

    fn user_text_line(text: &str) -> String {
        format!(r#"{{"type":"user","message":{{"role":"user","content":"{text}"}}}}"#)
    }

    #[test]
    fn last_ai_title_wins_over_the_earlier_ones() {
        let raw = [
            ai_title_line("First guess", SESSION),
            ai_title_line("Refined title", SESSION),
        ]
        .join("\n");

        assert_eq!(
            transcript_title(&raw, Some(SESSION)).as_deref(),
            Some("Refined title")
        );
    }

    #[test]
    fn ai_titles_of_other_sessions_are_ignored() {
        let raw = [
            ai_title_line("Mine", SESSION),
            ai_title_line("Someone else", "other-session"),
        ]
        .join("\n");

        assert_eq!(
            transcript_title(&raw, Some(SESSION)).as_deref(),
            Some("Mine")
        );
    }

    #[test]
    fn the_last_ai_title_is_taken_when_no_session_id_is_known() {
        let raw = [
            ai_title_line("Mine", SESSION),
            ai_title_line("Someone else", "other-session"),
        ]
        .join("\n");

        assert_eq!(
            transcript_title(&raw, None).as_deref(),
            Some("Someone else")
        );
    }

    #[test]
    fn falls_back_to_the_first_user_text_without_an_ai_title() {
        let raw = [
            r#"{"type":"summary","summary":"ignored"}"#.to_owned(),
            user_text_line("Fix the widget label"),
            user_text_line("And then ship it"),
        ]
        .join("\n");

        assert_eq!(
            transcript_title(&raw, Some(SESSION)).as_deref(),
            Some("Fix the widget label")
        );
    }

    #[test]
    fn user_texts_that_open_with_a_tag_are_skipped() {
        let raw = [
            user_text_line("<objective>"),
            user_text_line("  <command-name>/commit</command-name>"),
            user_text_line("Real question"),
        ]
        .join("\n");

        assert_eq!(
            transcript_title(&raw, Some(SESSION)).as_deref(),
            Some("Real question")
        );
    }

    #[test]
    fn reads_user_text_from_string_and_block_content() {
        let string_form = user_text_line("Plain string prompt");
        let block_form = concat!(
            r#"{"type":"user","message":{"role":"user","content":["#,
            r#"{"type":"tool_result","tool_use_id":"t","content":"ignored"},"#,
            r#"{"type":"text","text":"Block prompt"}]}}"#
        );

        assert_eq!(
            transcript_title(&string_form, None).as_deref(),
            Some("Plain string prompt")
        );
        assert_eq!(
            transcript_title(block_form, None).as_deref(),
            Some("Block prompt")
        );
    }

    #[test]
    fn normalizes_to_the_first_line_and_caps_the_length() {
        assert_eq!(
            normalize_title("  Keep this\nDrop that  ").as_deref(),
            Some("Keep this")
        );
        assert_eq!(normalize_title("   \n  "), None);
        assert_eq!(normalize_title(""), None);

        let long = "é".repeat(250);
        let capped = normalize_title(&long).unwrap_or_default();
        assert_eq!(capped.chars().count(), TITLE_LIMIT);
    }

    #[test]
    fn a_missing_transcript_yields_no_title() {
        assert_eq!(
            claude_session_title(Path::new("/nonexistent/transcript.jsonl"), Some(SESSION)),
            None
        );
    }

    #[test]
    fn finds_a_codex_rollout_by_session_id_suffix() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let day = dir.path().join("sessions/2026/08/16");
        fs::create_dir_all(&day)?;
        fs::write(
            day.join("rollout-2026-08-16T16-43-45-01a00b07-56bb-7ac0-a23d-fa8c29d778ee.jsonl"),
            "",
        )?;
        fs::write(
            day.join("rollout-2026-08-16T09-00-00-other-session.jsonl"),
            "",
        )?;

        let found = find_rollout(
            &dir.path().join("sessions"),
            "01a00b07-56bb-7ac0-a23d-fa8c29d778ee",
        );

        assert_eq!(
            found.as_deref().and_then(Path::file_name),
            Some("rollout-2026-08-16T16-43-45-01a00b07-56bb-7ac0-a23d-fa8c29d778ee.jsonl".as_ref())
        );
        Ok(())
    }

    #[test]
    fn extracts_the_first_codex_user_message() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let day = dir.path().join("sessions/2026/08/16");
        fs::create_dir_all(&day)?;
        fs::write(
            day.join("rollout-2026-08-16T16-43-45-session-7.jsonl"),
            concat!(
                r#"{"type":"session_meta","payload":{"id":"session-7"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"task_started"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","message":"<user_instructions>"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","message":"Label events by session\nsecond line"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"user_message","message":"Later turn"}}"#,
            ),
        )?;

        assert_eq!(
            codex_session_title(&dir.path().join("sessions"), "session-7").as_deref(),
            Some("Label events by session")
        );
        Ok(())
    }

    #[test]
    fn a_missing_rollout_yields_no_title() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        assert_eq!(codex_session_title(dir.path(), "session-7"), None);
        assert_eq!(
            codex_session_title(&dir.path().join("absent"), "session-7"),
            None
        );
        Ok(())
    }
}
