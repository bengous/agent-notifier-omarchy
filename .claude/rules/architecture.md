---
paths:
  - "src/**"
  - "tests/**"
---

# Architecture

Concept folders; hexagons at the file level — pure core files next to adapter files, never `domain/`/`adapters/` subfolders; one composition shell; one shared subprocess mechanism.

```
src/
├── main.rs         entry; hook exit-code policy (test-locked)
├── exec.rs         shared subprocess + timeout; importable by adapter files only
├── app/            composition shell — sequences, never decides
│   ├── cli.rs      argv → CliCommand, parsed once
│   ├── deps.rs     the world run() needs (state path, clock, stdin, windows, alert) + its system adapter
│   ├── mod.rs      run(cmd, deps) dispatch
│   ├── capture.rs  hooks: intake → window (workspace passed by value) → event → store → alert
│   ├── query.rs    the printing commands; mark-read-on-focus is a named fn here
│   ├── focus.rs    the state-changing commands: focus-*, mark-read, clear-*, prune-stale
│   └── watch.rs    daemon entry; the loop itself lives in the window adapter
├── event/          domain core — pure except store.rs
│   ├── model.rs    frozen events.json v1 contract: data + invariants, zero policy
│   ├── lifecycle.rs  born → visible → read → dead: capture?, dedupe, prune, cap; liveness injected as a view
│   ├── agent.rs    typed agent identity (enum) + display name
│   └── store.rs    persistence adapter: lock, atomic write, quarantine, skip-write-when-unchanged
├── intake/         capturing a completion
│   ├── build.rs    THE unified event builder (pure)
│   ├── git.rs      git adapter (project root, repository key, branch)
│   ├── id.rs       urandom adapter
│   ├── session_title.rs  session-title lookup in agent session files (fs)
│   └── agents/     one file per agent, data only (input fields, session_id chain, title source) — never assembly
├── window/         source window (existing / focused / live)
│   ├── resolve.rs  pure core: ranking, workspace resolution from injected data
│   ├── hyprland.rs the entire frontier: hyprctl, socket, reconnection, focus — "active" appears verbatim only here
│   └── proc.rs     /proc adapter: liveness, pid_chain
├── display/        pure projection for the widget (split label/status/popup when it grows)
└── alert/          alert(title, body); no domain dependency; the audio fallback is internal
```

Import edges, enforced by `tests/architecture.rs`:

- Allowed: main → app; app → every concept; {intake, window} → {event types, exec}; display → event; alert → exec; event → nothing.
- Forbidden: event → anything; anything → app; intake ↔ window; alert → {event, display}; any pure core → exec; app → {exec, std::fs}.
- Edges that still break these rules live in the `PLANNED_DEBT` list of `tests/architecture.rs`, each tagged with the deepening that removes it. The list only shrinks: a stale entry fails the test, a new one needs a reason.

Guardrails:

- app handlers sequence only — any new decision is a pure function in a concept core, tested there.
- Adapters are `pub(in crate::<concept>)`; `unreachable_pub = deny` stays in `[lints.rust]`.
- No trait while a seam has a single adapter. `app::Deps` is the one seam with two — the system and the fake every end-to-end test drives — so it is a trait; a new dependency joins it instead of growing a second seam.
- Escape hatch if hard enforcement is ever needed: a two-crate workspace (pure domain lib + binary with adapters), never one crate per concept.
