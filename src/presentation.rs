use chrono::DateTime;
use serde::Serialize;

use crate::state::{dedupe_events, AgentEvent, EventStatus};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DisplayAgentEvent {
    #[serde(flatten)]
    pub(crate) event: AgentEvent,
    #[serde(rename = "displayLabel")]
    pub(crate) display_label: String,
    #[serde(rename = "displayCreatedAt")]
    pub(crate) display_created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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

pub(crate) fn display_state_from_events(version: u8, events: Vec<AgentEvent>) -> DisplayState {
    DisplayState {
        version,
        events: dedupe_events(events)
            .into_iter()
            .map(|event| DisplayAgentEvent {
                display_label: event_label(&event),
                display_created_at: format_created_at(&event.created_at),
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
