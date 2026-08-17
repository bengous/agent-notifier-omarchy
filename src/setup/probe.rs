use std::env;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::event::Agent;
use crate::setup::state::{harness_report, HarnessFacts, SetupReport};

const HARNESSES: [Agent; 3] = [Agent::Claude, Agent::Codex, Agent::Pi];

pub(crate) fn gather_setup_probe(listener_live: bool) -> SetupReport {
    SetupReport {
        version: 1,
        binary_on_path: program_is_on_path("agent-notifier"),
        listener_live,
        harnesses: HARNESSES
            .into_iter()
            .map(|agent| harness_report(&harness_facts(agent)))
            .collect(),
    }
}

fn harness_facts(agent: Agent) -> HarnessFacts {
    let home = env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from);
    let config_path = config_path_for(agent, home.as_deref());
    let hook_command = fs::read_to_string(&config_path)
        .ok()
        .and_then(|raw| hook_command_from(agent, &raw));
    let hook_command_resolves = hook_command.as_deref().is_some_and(|command| {
        hook_program_resolves(command, home.as_deref(), &program_is_on_path, &|path| {
            path.is_file()
        })
    });
    HarnessFacts {
        agent,
        config_path: config_path.to_string_lossy().into_owned(),
        harness_on_path: program_is_on_path(agent.id()),
        config_exists: config_path.exists(),
        hook_command,
        hook_command_resolves,
    }
}

fn config_path_for(agent: Agent, home: Option<&Path>) -> PathBuf {
    let home_dir = home.unwrap_or_else(|| Path::new(""));
    match agent {
        Agent::Claude => home_dir.join(".claude/settings.json"),
        Agent::Codex => codex_config_path_from(env::var_os("CODEX_HOME").as_deref(), home),
        Agent::Pi => home_dir.join(".pi/agent/extensions/agent-notifier.ts"),
    }
}

// Duplicated from the CODEX_HOME resolution in intake::session_title rather
// than imported: setup and intake share no edge in the module graph.
fn codex_config_path_from(codex_home: Option<&OsStr>, home: Option<&Path>) -> PathBuf {
    codex_home
        .filter(|dir| !dir.is_empty())
        .map_or_else(
            || home.unwrap_or_else(|| Path::new("")).join(".codex"),
            PathBuf::from,
        )
        .join("config.toml")
}

fn hook_command_from(agent: Agent, raw: &str) -> Option<String> {
    match agent {
        Agent::Claude => claude_hook_command_from(raw),
        Agent::Codex => codex_hook_command_from(raw),
        Agent::Pi => marker_command(raw, "pi-hook"),
    }
}

fn claude_hook_command_from(raw: &str) -> Option<String> {
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(raw) else {
        return marker_command(raw, "claude-hook");
    };
    settings["hooks"]["Stop"]
        .as_array()?
        .iter()
        .flat_map(|matcher| matcher["hooks"].as_array().into_iter().flatten())
        .filter_map(|hook| hook["command"].as_str())
        .find(|command| command.contains("agent-notifier"))
        .map(str::to_owned)
}

fn codex_hook_command_from(raw: &str) -> Option<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("command"))
        .filter_map(|rest| rest.trim_start().strip_prefix('='))
        .filter_map(|value| quoted(value.trim()))
        .find(|command| command.contains("agent-notifier"))
}

fn quoted(value: &str) -> Option<String> {
    Some(value.strip_prefix('"')?.strip_suffix('"')?.to_owned())
}

fn marker_command(raw: &str, subcommand: &str) -> Option<String> {
    let marker = format!("agent-notifier {subcommand}");
    let before = &raw[..raw.find(&marker)?];
    Some(format!("{}{marker}", path_prefix(before)))
}

fn path_prefix(before: &str) -> &str {
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, character)| is_path_character(*character))
        .last()
        .map_or(before.len(), |(index, _)| index);
    &before[start..]
}

fn is_path_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '/' | '.' | '_' | '-' | '~')
}

fn hook_program_resolves(
    command: &str,
    home: Option<&Path>,
    on_path: &dyn Fn(&str) -> bool,
    exists: &dyn Fn(&Path) -> bool,
) -> bool {
    let Some(program) = command.split_whitespace().next() else {
        return false;
    };
    if program.contains('/') {
        exists(&expand_home(program, home))
    } else {
        on_path(program)
    }
}

fn expand_home(program: &str, home: Option<&Path>) -> PathBuf {
    program.strip_prefix("~/").map_or_else(
        || PathBuf::from(program),
        |rest| home.unwrap_or_else(|| Path::new("")).join(rest),
    )
}

fn program_is_on_path(program: &str) -> bool {
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|dir| is_executable_file(&dir.join(program)))
    })
}

fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests;
