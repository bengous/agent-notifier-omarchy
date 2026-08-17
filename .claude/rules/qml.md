---
paths:
  - "BarWidget.qml"
  - "components/**"
  - "js/**"
  - "Theme.qml"
---

# Widget conventions

- `BarWidget.qml` is assembly only (target < ~150 lines); every new piece of UI is a new component file in `components/`.
- Tokens: omarchy `Style.*` is the only source for sizes and typography — never duplicated. `Theme.qml` holds only what `Style.*` lacks: per-agent brand colors, animation durations, refresh intervals, derived color factors. Zero magic numbers inside components.
- Shared JS: one file per concept in `js/` (`js/time.js` first); extract at the second consumer; prefer native Qt/JS APIs before writing a helper; never `lib.js`/`utils.js`/`helpers.js`.
- Coerce every external datum at the parse boundary (`String()`, defaults under try/catch); report errors with `console.warn("agent-notifier", ...)`.
- The Rust-side widget-contract test lists exactly the JSON keys the QML reads — extend it with every new key you consume, and nothing else.
- `tests/widget-harness/` photographs the real widget under a nested Hyprland; it is a local tool and never a CI gate. Its invariants: target quickshell by `--pid` only; `QT_QPA_PLATFORM=wayland` always; read the nested monitor name and size back from `hyprctl -j`, never assume the mode you asked for.
