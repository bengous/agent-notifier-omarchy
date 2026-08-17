use serde_json::{Map, Value};
use std::fmt;

use crate::event::Agent;
use crate::setup::probe::{claude_hook_command_from, codex_hook_command_from};
use crate::setup::render::{CLAUDE_HOOK_BLOCK, CODEX_HOOK_BLOCK};

/// The harnesses `setup` writes. Pi is absent by decision: its extension is
/// user code, so a Pi wiring target must not be representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WireTarget {
    Claude,
    Codex,
}

impl WireTarget {
    pub(crate) fn from_harness_id(id: &str) -> Option<Self> {
        [Self::Claude, Self::Codex]
            .into_iter()
            .find(|target| target.agent().id() == id)
    }

    pub(crate) const fn agent(self) -> Agent {
        match self {
            Self::Claude => Agent::Claude,
            Self::Codex => Agent::Codex,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WireAction {
    Wire,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WireChange {
    Wired,
    AlreadyWired,
    Removed,
    NothingToRemove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WireOutcome {
    pub(crate) config_path: String,
    pub(crate) change: WireChange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::setup) enum WirePlan {
    AlreadyDone(WireChange),
    Write(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WireError {
    HarnessAbsent { harness: &'static str },
    HarnessDirAbsent { harness: &'static str, dir: String },
    BinaryUnresolvable,
    ConfigUnparsable { path: String, block: &'static str },
    ForeignContent { path: String, block: &'static str },
    LockHeld { path: String, pid: String },
    HookDidNotVerify { path: String },
}

impl fmt::Display for WireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HarnessAbsent { harness } => {
                write!(formatter, "{harness} is not on PATH; install it first")
            }
            Self::HarnessDirAbsent { harness, dir } => write!(
                formatter,
                "{dir} does not exist; run {harness} once, then run this again"
            ),
            Self::BinaryUnresolvable => write!(
                formatter,
                "agent-notifier is not on PATH; install the binary first (README, section Install)"
            ),
            Self::ConfigUnparsable { path, block } => write!(
                formatter,
                "cannot parse {path}; add this block by hand:\n\n{block}"
            ),
            Self::ForeignContent { path, block } => write!(
                formatter,
                "{path} holds a hand-edited agent-notifier block; remove it yourself, then run this again. This command writes:\n\n{block}"
            ),
            Self::LockHeld { path, pid } => write!(
                formatter,
                "another agent-notifier setup holds {path} (pid {pid})"
            ),
            Self::HookDidNotVerify { path } => write!(
                formatter,
                "wrote {path} but the hook does not read back; the previous file is restored"
            ),
        }
    }
}

impl std::error::Error for WireError {}

/// Why an editor refuses to produce a text. The path and the paste block that
/// turn it into an operator message belong to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::setup) enum EditRefusal {
    Unparsable,
    ForeignContent,
}

type Editor = fn(Option<&str>) -> Result<String, EditRefusal>;

/// Everything the engine knows about one harness. Adding a harness is one
/// variant, one spec, one pair of editors — the engine never matches a target.
#[derive(Debug)]
pub(in crate::setup) struct TargetSpec {
    pub(in crate::setup) agent: Agent,
    pub(in crate::setup) canonical_command: &'static str,
    pub(in crate::setup) block: &'static str,
    pub(in crate::setup) extract: fn(&str) -> Option<String>,
    with_hook: Editor,
    without_hook: Editor,
}

pub(in crate::setup) const fn target_spec(target: WireTarget) -> TargetSpec {
    match target {
        WireTarget::Claude => TargetSpec {
            agent: Agent::Claude,
            canonical_command: "agent-notifier claude-hook",
            block: CLAUDE_HOOK_BLOCK,
            extract: claude_hook_command_from,
            with_hook: claude_settings_with_hook,
            without_hook: claude_settings_without_hook,
        },
        WireTarget::Codex => TargetSpec {
            agent: Agent::Codex,
            canonical_command: "agent-notifier hook",
            block: CODEX_HOOK_BLOCK,
            extract: codex_hook_command_from,
            with_hook: codex_config_with_hook,
            without_hook: codex_config_without_hook,
        },
    }
}

pub(in crate::setup) struct WirePlanInput<'a> {
    pub(in crate::setup) spec: &'a TargetSpec,
    pub(in crate::setup) action: WireAction,
    pub(in crate::setup) config_path: &'a str,
    pub(in crate::setup) harness_on_path: bool,
    pub(in crate::setup) existing: Option<&'a str>,
    pub(in crate::setup) resolves: &'a dyn Fn(&str) -> bool,
}

/// The decision, taken before a single byte moves: an already wired config is
/// left alone, and a binary the hook could not reach fails here rather than
/// after a write-then-rollback cycle.
pub(in crate::setup) fn wire_plan(input: &WirePlanInput) -> Result<WirePlan, WireError> {
    let existing_command = input.existing.and_then(|raw| (input.spec.extract)(raw));
    match input.action {
        WireAction::Wire => {
            if !input.harness_on_path {
                return Err(WireError::HarnessAbsent {
                    harness: input.spec.agent.display_name(),
                });
            }
            if existing_command
                .as_deref()
                .is_some_and(|command| (input.resolves)(command))
            {
                return Ok(WirePlan::AlreadyDone(WireChange::AlreadyWired));
            }
            if !(input.resolves)(input.spec.canonical_command) {
                return Err(WireError::BinaryUnresolvable);
            }
            edited(input.spec.with_hook, input).map(WirePlan::Write)
        }
        WireAction::Remove => {
            if existing_command.is_none() {
                return Ok(WirePlan::AlreadyDone(WireChange::NothingToRemove));
            }
            edited(input.spec.without_hook, input).map(WirePlan::Write)
        }
    }
}

/// The validation the fs edge runs on the bytes it just wrote.
pub(in crate::setup) fn written_verifies(
    spec: &TargetSpec,
    action: WireAction,
    written: &str,
    resolves: &dyn Fn(&str) -> bool,
) -> bool {
    let command = (spec.extract)(written);
    match action {
        WireAction::Wire => command.is_some_and(|command| resolves(&command)),
        WireAction::Remove => command.is_none(),
    }
}

pub(crate) fn wire_line(target: WireTarget, outcome: &WireOutcome) -> String {
    let harness = target.agent().display_name();
    let path = &outcome.config_path;
    match outcome.change {
        WireChange::Wired => format!("{harness} wired ({path})"),
        WireChange::AlreadyWired => format!("{harness} already wired ({path})"),
        WireChange::Removed => format!("{harness} hook removed ({path})"),
        WireChange::NothingToRemove => format!("{harness} hook already absent ({path})"),
    }
}

fn edited(editor: Editor, input: &WirePlanInput) -> Result<String, WireError> {
    editor(input.existing).map_err(|refusal| {
        let path = input.config_path.to_owned();
        let block = input.spec.block;
        match refusal {
            EditRefusal::Unparsable => WireError::ConfigUnparsable { path, block },
            EditRefusal::ForeignContent => WireError::ForeignContent { path, block },
        }
    })
}

fn claude_settings_with_hook(existing: Option<&str>) -> Result<String, EditRefusal> {
    let mut settings = claude_settings(existing)?;
    let mut matchers = claude_stop_matchers(&settings)?;
    matchers.push(canonical_claude_matcher().ok_or(EditRefusal::Unparsable)?);
    set_claude_stop(&mut settings, matchers);
    render_json(&settings)
}

fn claude_settings_without_hook(existing: Option<&str>) -> Result<String, EditRefusal> {
    let mut settings = claude_settings(existing)?;
    let matchers = claude_stop_matchers(&settings)?;
    set_claude_stop(&mut settings, matchers);
    render_json(&settings)
}

fn claude_settings(existing: Option<&str>) -> Result<Value, EditRefusal> {
    let Some(raw) = existing else {
        return Ok(Value::Object(Map::new()));
    };
    let settings = serde_json::from_str::<Value>(raw).map_err(|_error| EditRefusal::Unparsable)?;
    if settings.is_object() {
        Ok(settings)
    } else {
        Err(EditRefusal::Unparsable)
    }
}

/// The `Stop` matchers of the settings, with every agent-notifier hook gone and
/// every matcher this removal emptied dropped with it.
fn claude_stop_matchers(settings: &Value) -> Result<Vec<Value>, EditRefusal> {
    let hooks = match settings.get("hooks") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Object(hooks)) => hooks,
        Some(_) => return Err(EditRefusal::Unparsable),
    };
    let matchers = match hooks.get("Stop") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(matchers)) => matchers,
        Some(_) => return Err(EditRefusal::Unparsable),
    };
    Ok(matchers.iter().filter_map(matcher_without_us).collect())
}

fn matcher_without_us(matcher: &Value) -> Option<Value> {
    let Some(fields) = matcher.as_object() else {
        return Some(matcher.clone());
    };
    let Some(Value::Array(hooks)) = fields.get("hooks") else {
        return Some(matcher.clone());
    };
    let kept = hooks
        .iter()
        .filter(|hook| !is_our_hook(hook))
        .cloned()
        .collect::<Vec<_>>();
    if kept.len() == hooks.len() {
        return Some(matcher.clone());
    }
    if kept.is_empty() {
        return None;
    }
    let mut fields = fields.clone();
    fields.insert("hooks".to_owned(), Value::Array(kept));
    Some(Value::Object(fields))
}

fn is_our_hook(hook: &Value) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains("agent-notifier"))
}

fn set_claude_stop(settings: &mut Value, matchers: Vec<Value>) {
    let Some(fields) = settings.as_object_mut() else {
        return;
    };
    if matchers.is_empty() {
        let emptied = fields
            .get_mut("hooks")
            .and_then(Value::as_object_mut)
            .map(|hooks| {
                hooks.remove("Stop");
                hooks.is_empty()
            });
        if emptied == Some(true) {
            fields.remove("hooks");
        }
        return;
    }
    let hooks = fields
        .entry("hooks".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(hooks) = hooks.as_object_mut() {
        hooks.insert("Stop".to_owned(), Value::Array(matchers));
    }
}

fn canonical_claude_matcher() -> Option<Value> {
    serde_json::from_str::<Value>(CLAUDE_HOOK_BLOCK)
        .ok()?
        .get("Stop")?
        .as_array()?
        .first()
        .cloned()
}

fn render_json(settings: &Value) -> Result<String, EditRefusal> {
    serde_json::to_string_pretty(settings)
        .map(|json| format!("{json}\n"))
        .map_err(|_error| EditRefusal::Unparsable)
}

fn codex_config_with_hook(existing: Option<&str>) -> Result<String, EditRefusal> {
    let Some(raw) = existing else {
        return Ok(format!("{CODEX_HOOK_BLOCK}\n"));
    };
    let stripped = codex_config_without_our_block(raw)?;
    Ok(codex_block_appended(&stripped))
}

fn codex_config_without_hook(existing: Option<&str>) -> Result<String, EditRefusal> {
    codex_config_without_our_block(existing.unwrap_or_default())
}

/// A block this command wrote is removed byte for byte. Anything else carrying
/// the marker was written by hand, and a text edit is not the way to touch it.
fn codex_config_without_our_block(raw: &str) -> Result<String, EditRefusal> {
    let Some(start) = raw.find(CODEX_HOOK_BLOCK) else {
        return if codex_hook_command_from(raw).is_some() {
            Err(EditRefusal::ForeignContent)
        } else {
            Ok(raw.to_owned())
        };
    };
    let mut end = start + CODEX_HOOK_BLOCK.len();
    if raw[end..].starts_with('\n') {
        end += 1;
    }
    let head = &raw[..start];
    let start = head
        .strip_suffix("\n\n")
        .map_or(start, |before| before.len() + 1);
    Ok(format!("{}{}", &raw[..start], &raw[end..]))
}

/// A `[[hooks.Stop]]` header at the end of a TOML file is always valid, so the
/// block is appended rather than parsed into place.
fn codex_block_appended(raw: &str) -> String {
    if raw.trim().is_empty() {
        return format!("{CODEX_HOOK_BLOCK}\n");
    }
    let head = raw.strip_suffix('\n').unwrap_or(raw);
    format!("{head}\n\n{CODEX_HOOK_BLOCK}\n")
}

#[cfg(test)]
mod tests;
