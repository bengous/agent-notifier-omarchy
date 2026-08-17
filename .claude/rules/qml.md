---
paths:
  - "BarWidget.qml"
  - "components/**"
  - "js/**"
  - "Theme.qml"
---

# Widget conventions

- `BarWidget.qml` is assembly only — state wiring, CLI plumbing, and declarative composition of `components/`; zero domain logic in the entry point. Every new piece of UI is a new component file in `components/`, never growth of `BarWidget.qml`.
- Tokens: omarchy `Style.*` is the only source for sizes and typography — never duplicated. `Theme.qml` holds only what `Style.*` lacks: per-agent brand colors, animation durations, refresh intervals, derived color factors. Zero magic numbers inside components.
- `Theme.qml` is a singleton declared in the root `qmldir`, and qmldir singletons resolve only through an explicit directory import: `import "."` in `BarWidget.qml`, `import ".."` in `components/`. The `qmldir` also names `BarWidget` so directory imports of the plugin (the harness shell) keep seeing it.
- Shared JS: one file per concept in `js/` (`js/time.js` first); extract at the second consumer; prefer native Qt/JS APIs before writing a helper; never `lib.js`/`utils.js`/`helpers.js`.
- Coerce every external datum at the parse boundary (`String()`, defaults under try/catch); report errors with `console.warn("agent-notifier", ...)`.
- The widget contract lists exactly the JSON keys the QML reads, and both sides hold it: `display_state_exposes_exactly_the_keys_the_widget_reads` in Rust, `tests/qml/DisplayContract.qml` in QML. A new key you consume joins both, and nothing else does.
- `tests/qml/` is the deterministic gate: `run.sh` feeds a real `list-display-json` to the Qt 6 `qmltestrunner`, and pure-JS tests of `js/` land there too.
- `tests/widget-harness/` photographs the real widget under a nested Hyprland; it is a local tool and never a CI gate. Its invariants: target quickshell by `--pid` only; `QT_QPA_PLATFORM=wayland` always; read the nested monitor name and size back from `hyprctl -j`, never assume the mode you asked for.
