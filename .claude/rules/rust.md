---
paths:
  - "src/**"
---

# Rust conventions

- serde: `rename_all = "camelCase"` at container level; `#[serde(alias)]` over twin fields — a JSON carrying both spellings must fail; every new field gets `skip_serializing_if` plus the additive test pair ("serializes additively", "parses v1 without").
- External commands only via the process helpers (`command_output`/`run_command`, `DEFAULT_TIMEOUT`); never `Command` directly (build.rs excepted).
- Errors: `io::Result` up to main; swallowing is allowed only as a documented choice; stderr prefixed `agent-notifier: `.
- Naming: the Vocabulary section of `AGENTS.md` is authoritative. No domain use of "active" or "acknowledged" outside the Hyprland adapter (grep before commit).
- Tests live with the module they test, in one layout everywhere: `#[cfg(test)] mod tests;` plus a `tests.rs` file, never an inline block. Command-level behavior stays with app. Event fixtures shared by several concepts live in `src/test_fixtures.rs`; a test file may cross concept boundaries, production code may not. Fixtures are deterministic — no fallback clocks (`unwrap_or_else(Utc::now)` in a test is a bug).
