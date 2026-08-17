---
paths:
  - "widget/**"
---

# Widget conventions

- The whole front lives in `widget/`; the plugin root holds `manifest.json` and `assets/`, so a component reaches a mark at `../../assets/<agent>.svg`.
- `widget/BarWidget.qml` is assembly only — state wiring, CLI plumbing, and declarative composition of `widget/components/`; zero domain logic in the entry point. Every new piece of UI is a new component file in `widget/components/`, never growth of `widget/BarWidget.qml`.
- Tokens: omarchy `Style.*` is the only source for sizes and typography — never duplicated. `widget/Theme.qml` holds only what `Style.*` lacks: per-agent brand colors, animation durations, refresh intervals, derived color factors. Zero magic numbers inside components.
- `Theme.qml` is a singleton declared in `widget/qmldir`, and qmldir singletons resolve only through an explicit directory import: `import "."` in `BarWidget.qml`, `import ".."` in `components/`. The `qmldir` also names `BarWidget` so directory imports of `widget/` (the harness shell) keep seeing it.
- Shared JS: one file per concept in `widget/js/` (`time.js` first); extract at the second consumer; prefer native Qt/JS APIs before writing a helper; never `lib.js`/`utils.js`/`helpers.js`.
- Coerce every external datum at the parse boundary (`String()`, defaults under try/catch); report errors with `console.warn("agent-notifier", ...)`.
- The widget contract lists exactly the JSON keys the QML reads, and both sides hold it: `display_state_exposes_exactly_the_keys_the_widget_reads` in Rust, `tests/qml/DisplayContract.qml` in QML. A new key you consume joins both, and nothing else does.
- `tests/qml/` is the deterministic gate: `run.sh` feeds a real `list-display-json` to the Qt 6 `qmltestrunner`, and pure-JS tests of `widget/js/` land there too.
- `tests/widget-harness/` photographs the real widget under a nested Hyprland; it is a local tool and never a CI gate. Its invariants: target quickshell by `--pid` only; `QT_QPA_PLATFORM=wayland` always; read the nested monitor name and size back from `hyprctl -j`, never assume the mode you asked for.
- Harness debug traps: quickshell's auto-reload does not follow the `AgentNotifier` symlink, so instrumenting a widget file in a live harness changes nothing — to debug a signal sequence, write a minimal standalone shell and run it with `qs -p`. A capture at the host's monitor mode (say 5120x1440 instead of 1280x800) means the float never settled: re-run before suspecting the widget.
