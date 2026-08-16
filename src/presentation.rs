use chrono::DateTime;
use serde::Serialize;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;

use crate::state::{dedupe_events, AgentEvent, EventStatus};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct DisplayAgentEvent {
    #[serde(flatten)]
    pub(crate) event: AgentEvent,
    #[serde(rename = "displayLabel")]
    pub(crate) display_label: String,
    #[serde(rename = "displayCreatedAt")]
    pub(crate) display_created_at: String,
    #[serde(rename = "displayProject")]
    pub(crate) display_project: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct DisplayState {
    pub(crate) version: u8,
    pub(crate) events: Vec<DisplayAgentEvent>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct StatusOutput {
    pub(crate) text: String,
    pub(crate) tooltip: String,
    pub(crate) class: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct BuildInfo {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) commit: String,
    pub(crate) dirty: bool,
    #[serde(rename = "commitDate")]
    pub(crate) commit_date: String,
}

pub(crate) fn build_info(
    name: &str,
    version: &str,
    commit: &str,
    dirty: &str,
    commit_date: &str,
) -> BuildInfo {
    BuildInfo {
        name: name.to_owned(),
        version: version.to_owned(),
        commit: commit.to_owned(),
        dirty: dirty == "true",
        commit_date: commit_date.to_owned(),
    }
}

pub(crate) fn clean_workspace_title(title: &str) -> String {
    title
        .trim_start_matches(|character: char| {
            character.is_whitespace()
                || matches!(character,
                    '\u{2800}'..='\u{28ff}'
                        | '\u{25d0}'..='\u{25d3}'
                        | '\u{25dc}'..='\u{25e1}'
                        | '\u{25f4}'..='\u{25f7}'
                        | '\u{2722}'
                        | '\u{2733}'
                        | '\u{2736}'
                        | '\u{273b}'
                        | '\u{273d}'
                        | '\u{2743}'
                        | '\u{2749}')
        })
        .trim()
        .to_owned()
}

pub(crate) fn event_label(event: &AgentEvent) -> String {
    if let Some(title) = event
        .session_title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
    {
        return title.to_owned();
    }
    if let Some(workspace) = &event.workspace {
        let title = clean_workspace_title(&workspace.title);
        if !title.is_empty() {
            return title;
        }
        if let Some(branch_name) = &event.branch_name {
            return format!("{} | {branch_name}", event.project_name);
        }
        return format!("{}@{}", event.project_name, workspace.name);
    }
    if let Some(branch_name) = &event.branch_name {
        return format!("{} | {branch_name}", event.project_name);
    }
    event.project_name.clone()
}

fn format_tooltip(events: &[AgentEvent]) -> String {
    if events.is_empty() {
        return "No agent completions".to_owned();
    }
    events
        .iter()
        .map(|event| {
            let label = event_label(event);
            let clean_title = event
                .workspace
                .as_ref()
                .map(|workspace| clean_workspace_title(&workspace.title))
                .unwrap_or_default();
            let suffix = if clean_title.is_empty() || clean_title == label {
                String::new()
            } else {
                format!(" - {clean_title}")
            };
            format!("{label} {}{suffix}", format_created_at(&event.created_at))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_created_at(created_at: &str) -> String {
    DateTime::parse_from_rfc3339(created_at).map_or_else(
        |_| created_at.to_owned(),
        |timestamp| timestamp.format("%b %-d, %Y %-I:%M %p UTC").to_string(),
    )
}

pub(crate) fn format_agent_button(unread_count: usize) -> String {
    if unread_count == 0 {
        return "agents".to_owned();
    }
    format!("agents 󰂚 {unread_count}")
}

pub(crate) fn status_output(events: &[AgentEvent]) -> StatusOutput {
    let unread = events
        .iter()
        .filter(|event| event.status == EventStatus::Unread)
        .cloned()
        .collect::<Vec<_>>();
    StatusOutput {
        text: format_agent_button(unread.len()),
        tooltip: format_tooltip(&unread),
        class: if unread.is_empty() {
            "empty".to_owned()
        } else {
            "unread".to_owned()
        },
    }
}

fn project_group_key(event: &AgentEvent) -> &str {
    event
        .project_key
        .as_deref()
        .filter(|key| !key.is_empty())
        .unwrap_or(&event.project_path)
}

/// Newest-first input keeps every group ordered by its newest event.
fn group_by_project(events: Vec<AgentEvent>) -> Vec<AgentEvent> {
    let mut groups: Vec<(String, Vec<AgentEvent>)> = Vec::new();
    for event in events {
        let key = project_group_key(&event).to_owned();
        match groups.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, group)) => group.push(event),
            None => groups.push((key, vec![event])),
        }
    }
    groups.into_iter().flat_map(|(_, group)| group).collect()
}

fn project_key_name(key: &str, fallback: &str) -> String {
    Path::new(key)
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .map_or_else(|| fallback.to_owned(), str::to_owned)
}

fn with_parent_directory(key: &str, name: &str) -> String {
    Path::new(key)
        .parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .map_or_else(|| name.to_owned(), |parent| format!("{name} — {parent}"))
}

fn project_labels(events: &[AgentEvent]) -> HashMap<String, String> {
    let mut named: Vec<(String, String)> = Vec::new();
    for event in events {
        let key = project_group_key(event);
        if named.iter().any(|(existing, _)| existing.as_str() == key) {
            continue;
        }
        named.push((key.to_owned(), project_key_name(key, &event.project_name)));
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, name) in &named {
        *counts.entry(name.clone()).or_default() += 1;
    }
    named
        .into_iter()
        .map(|(key, name)| {
            let label = if counts.get(&name).is_some_and(|count| *count > 1) {
                with_parent_directory(&key, &name)
            } else {
                name
            };
            (key, label)
        })
        .collect()
}

pub(crate) fn display_state_from_events(version: u8, events: Vec<AgentEvent>) -> DisplayState {
    let grouped = group_by_project(dedupe_events(events));
    let labels = project_labels(&grouped);
    DisplayState {
        version,
        events: grouped
            .into_iter()
            .map(|event| DisplayAgentEvent {
                display_label: event_label(&event),
                display_created_at: format_created_at(&event.created_at),
                display_project: labels
                    .get(project_group_key(&event))
                    .cloned()
                    .unwrap_or_else(|| event.project_name.clone()),
                event,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_any_leading_spinner_glyph() {
        // U+28F8 is not in the historical blocklist.
        assert_eq!(clean_workspace_title("⣸ building"), "building");
        assert_eq!(clean_workspace_title("\u{28f8} building"), "building");
        assert_eq!(clean_workspace_title("◑ building"), "building");
        assert_eq!(clean_workspace_title("◜ building"), "building");
        assert_eq!(clean_workspace_title("◷ building"), "building");
        assert_eq!(clean_workspace_title("✻ building"), "building");
        assert_eq!(clean_workspace_title("  plain title  "), "plain title");
        assert_eq!(clean_workspace_title("~/dotfiles"), "~/dotfiles");
    }
}
