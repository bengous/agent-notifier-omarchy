# Agent Notifier

Rust CLI + Quickshell bar widget for Omarchy: captures when a coding agent (Claude Code, Codex, Pi) finishes a turn, shows unread completions in the bar, and focuses the window that finished. Omarchy-only by design: window focus goes through Omarchy's `hl.dsp.focus` dispatcher, with no bare-Hyprland fallback.

## Vocabulary

One concept, one name — in code, docs, and tests:

- **existing window**: present in the compositor. **focused window**: holds input focus. Never say "active"; that word appears only verbatim at the Hyprland protocol boundary.
- **read / unread**: event status; the verb is "mark read". Never "acknowledged".
- **source window**: the window an agent ran in. **live** describes processes, never windows.
- **event**: one persisted completion. **alert**: the one-shot notification + sound.
- **project**: the repository grouping events; worktrees of one repository merge.

## Architecture invariants

- Decisions are pure state → state functions; I/O executes at the edge (`FocusOutcome` is the reference pattern). New policy is born in a pure core and tested there.
- One system boundary = one module: all Hyprland talk (commands, protocol, reconnection) belongs to the Hyprland module; /proc and subprocess spawning go only through the process helpers (timeout mandatory).
- Command handlers sequence; they never branch on domain logic.
- A read command never writes — except the mark-read-on-focus contract: `status-json` and `list-display-json` deliberately mark the focused window's events read.
- No trait while a seam has a single adapter. No per-agent pipeline: one event builder; agents vary in data only.

## Contracts

- Public surface: CLI names/args/exit codes, `status-json` and `list-display-json` shapes, `events.json` v1, the IPC target. The schema evolves additively only; every new serialized field ships with a "serializes additively" and a "parses v1 without" test.
- A hook exit never blocks an agent turn; the exit-code policy in `main.rs` is test-locked.
- Pre-v1: breaking changes are allowed; update the README in the same commit. No compat shims — fail with a clear error.
- `TODO(contract):` marks a documented command with no known consumer: retire or test it before v1.

## Style

- Comments: only irreplaceable "why" (external constraint, workaround, contract policy). No paraphrase, no history. A comment describing observable behavior becomes a test or cites its source.
- `pub(crate)` by default; English everywhere; stderr messages prefixed `agent-notifier: `.
- Tests: behavior-sentence names, `Result<_, Box<dyn Error>>`, layered fixtures.
- QML: sizes and typography via omarchy `Style.*` tokens; local tokens live in one place; coerce all external data defensively; one component per file; shared JS in `js/<concept>.js` — never `lib`/`utils`; prefer native Qt/JS APIs over hand-rolled helpers.

## Keeping these docs honest

`AGENTS.md` and `.claude/rules/` are living documents: when your change makes a rule stale, fix the doc in the same commit; when a new convention crystallizes, add a rule. They guide upfront — never let them re-explain code that should document itself.
