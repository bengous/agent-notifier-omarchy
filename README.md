# Agent Notifier for Omarchy

An agent completion center for the Omarchy bar.

`agent-notifier` turns Codex, Claude Code, and Pi completion hooks into desktop
alerts and into a bar widget that lists the sessions still waiting for you. Each
entry focuses its source window, so a finished agent is one click away.

**Linux + Omarchy (Hyprland) only.**

## Install

Two steps: the binary, then the plugin.

### 1. The binary

```sh
cargo install --path .
```

`cargo install` puts `agent-notifier` in `~/.cargo/bin`. The completion sound is
not installed this way; set `AGENT_NOTIFIER_SOUND_FILE` to
`data/agent-complete.mp3`, or use the installer instead:

```sh
./install.sh
```

`install.sh` builds the release binary and installs it atomically after its
runtime data. It defaults to `/usr/local/bin/agent-notifier` and
`/usr/local/share/agent-notifier`, and accepts `PREFIX`, `DESTDIR`, `BINDIR`,
and `BIN_NAME`.

No Rust toolchain? Every
[release](https://github.com/bengous/agent-notifier-omarchy/releases) carries an
`x86_64` and an `aarch64` Linux archive, each with a `.sha256` checksum file.
Unpack the archive for your machine and put `agent-notifier` on your `PATH`.

The bar widget calls `agent-notifier` through `PATH`, so the binary directory
must be on the `PATH` of your Omarchy session.

### 2. The plugin

```sh
omarchy plugin add https://github.com/bengous/agent-notifier-omarchy.git --enable
```

Omarchy clones this repository as-is into
`~/.config/omarchy/plugins/io.github.bengous.agent-notifier/` and reads
`manifest.json` at its root. The clone carries the Rust sources with it; they
are inert. The widget never runs code from the clone — it calls the binary you
installed in step 1.

## Remove

```sh
omarchy plugin remove io.github.bengous.agent-notifier
```

Then drop the binary (`~/.cargo/bin/agent-notifier`, or `/usr/local/bin/agent-notifier`
and `/usr/local/share/agent-notifier` after `install.sh`), the state directory
`$XDG_STATE_HOME/agent-notifier`, and the hook entries you added below.

## Plugin files

- `manifest.json` — plugin manifest (`kinds: ["bar-widget"]`, id `io.github.bengous.agent-notifier`).
- `widget/BarWidget.qml` — entry point: the bar icon with its unread badge, plus
  the popup assembled from `widget/components/`. It reads through
  `agent-notifier list-display-json` and `agent-notifier version-json`, and
  watches the state file at the `statePath` that `version-json` reports;
  clicking a row runs `focus-id`, and the popup buttons run `clear-read` and
  `clear-all`.
- `widget/components/` — one component per file: CLI process plumbing, event row,
  project section header, footer button, version block.
- `widget/Theme.qml` + `widget/qmldir` — the widget's local tokens (per-agent
  brand colors, animation and refresh timings) as a QML singleton; sizes and
  typography come from omarchy's `Style.*`.
- `widget/js/time.js` — relative/absolute time formatting, tested from
  `tests/qml/`.

Prefer a plain bar module over the plugin? `agent-notifier status-json` emits
`{"text","tooltip","class"}` for a `"type": "command"` entry in your
`~/.config/omarchy/shell.json` bar layout.

## Hook wiring

Every hook reads its payload on stdin and exits fast. A hook failure never
blocks an agent turn.

### Codex

In `~/.codex/config.toml`:

```toml
[[hooks.Stop]]

[[hooks.Stop.hooks]]
command = "agent-notifier hook"
statusMessage = "Recording Codex completion"
timeout = 5
type = "command"
```

### Claude Code

In `~/.claude/settings.json`, merge this into `hooks`:

```json
{
  "Stop": [
    {
      "hooks": [
        {
          "type": "command",
          "command": "agent-notifier claude-hook",
          "timeout": 5
        }
      ]
    }
  ]
}
```

`Stop` only, not `SubagentStop`: subagents do not raise notifications.

### Pi

In `~/.pi/agent/extensions/agent-notifier.ts`, listen to main-agent `agent_end`
and pipe the payload into `agent-notifier pi-hook`:

```ts
export default function (pi: ExtensionAPI) {
  pi.on("agent_end", async (_event, ctx) => {
    if (process.env.PI_SUBAGENT_CHILD === "1") return;

    await notifyAgentCompletion({
      cwd: ctx.cwd,
      ...readSessionMetadata(ctx),
    });
  });
}
```

### Focused window listener

Focusing a window by hand marks its events read. Start the listener from
`~/.config/hypr/autostart.lua`:

```lua
o.exec_on_start("/usr/local/bin/agent-notifier watch-focused-window")
```

Use the full path here: the Hyprland startup environment does not always carry
your shell `PATH`.

## Commands

| Command | Purpose |
|---|---|
| `hook`, `claude-hook`, `pi-hook` | Capture a completion from stdin |
| `status-json` | Print `{"text","tooltip","class"}` for the bar widget; `class` is `empty` or `unread` |
| `list-display-json` | Print the focusable events the widget renders |
| `version-json` | Print build metadata: `{"name","version","commit","dirty","commitDate","statePath"}` |
| `focus-id <event-id>` | Focus one event and mark it read |
| `mark-read <event-id>` | Mark one event read |
| `watch-focused-window` | Mark the focused window's events read, as a long-running listener |
| `clear-read`, `clear-all` | Remove read events, or all of them |
| `prune-stale` | Remove events whose source window is gone |

`--help` prints this list. `--version` prints the package version. Unknown
commands, extra arguments, and a missing event id exit 2. `focus-id` exits 1
when it reaches no window.

An event is visible only while its source window still exists. Completions from
the already-focused window are dropped: you are looking at them.

A single-process terminal, such as ghostty with `--gtk-single-instance`, gives
every window one pid. The source window is therefore not knowable from the pid
alone, so an event stores three things:

- `clientAddress`, the best guess, and `clientAddresses`, every window that can
  be the source, ranked. A click tries them in order.
- `sourceProcess`, the pid and start time of the window's own shell. The event
  dies with that window, not with the whole terminal application.

A click that lands on the best guess marks the event read. A click that falls
back to another window leaves it unread: the source was not reached.

## State

```text
$XDG_STATE_HOME/agent-notifier/events.json
```

The fallback is `$HOME/.local/state/agent-notifier/events.json`. Writes are
atomic and hold an advisory lock. Invalid state is renamed to
`events.json.corrupt-<timestamp>` and treated as empty. At most 50 events are
kept.

The binary owns this path: `version-json` reports the one it resolved as
`statePath`, and `statePath` is absent when neither `XDG_STATE_HOME` nor `HOME`
is set. Ask for it instead of deriving a second path from the environment.

`events.json` is an internal format. Read the state through `status-json` and
`list-display-json` instead.

A rewrite keeps the JSON keys this binary does not know, so fields written by a
newer binary survive it. That covers added keys only: a state file with another
`version` is quarantined as invalid. After an upgrade, restart every long-lived
process — above all the `watch-focused-window` listener — or its rewrites drop
the newer fields until you do.

## Environment

| Variable | Effect |
|---|---|
| `XDG_STATE_HOME`, `HOME` | Where `events.json` lives; see [State](#state) |
| `AGENT_NOTIFIER_SHARE_DIR` | Runtime data directory, instead of the one derived from the binary path |
| `AGENT_NOTIFIER_SOUND_DIR` | Directory holding `agent-complete.mp3`, instead of the runtime data directory |
| `AGENT_NOTIFIER_SOUND_FILE` | Full path of the completion sound; wins over both directories |
| `AGENT_NOTIFIER_SOUND` | `0` plays no sound |
| `PWD` | Working directory of a hook payload that carries none |
| `XDG_RUNTIME_DIR`, `HYPRLAND_INSTANCE_SIGNATURE` | Hyprland event socket of `watch-focused-window`; both come from the compositor |
| `CODEX_HOME` | Codex directory holding the sessions read for a session title |
| `CODEX_SESSION_ID`, `PI_SESSION_ID` | Session id of a hook payload that carries none |

## Dependencies

| Dependency | Requirement | Behaviour when missing |
|---|---|---|
| Omarchy, Hyprland, `hyprctl` | Required | Shipped with Omarchy; use elsewhere is unsupported |
| `notify-send` | Optional | No desktop pop-up; the event is still captured |
| `mpv`, `canberra-gtk-play` | Optional | No sound; both are tried in that order |
| `git` | Optional | No branch name; the cwd becomes the project path |
| Rust >= 1.89, Bash | Build only | - |

## Tests

`scripts/check-all` runs every gate the CI runs, and is the one command to run
before a commit:

| Layer | Command | Runs |
|---|---|---|
| Rust format, lints, tests | `scripts/check` | CI |
| Module graph | `scripts/check-modules` | CI |
| QML lint | `scripts/check-qml` | CI |
| Shell lint | `scripts/check-shell` | CI |
| Widget contract | `tests/qml/run.sh` | CI |
| Widget visual | `tests/widget-harness/run.sh` | Locally only |

The CI runs these same scripts, so a green `scripts/check-all` is a green CI.
`scripts/check-modules` needs `cargo install cargo-modules --locked`;
`scripts/check-qml` needs the Qt 6 `qmllint` and takes a `QMLLINT` override.
It reports neither the unresolvable `qs.*` imports of omarchy nor what they
make unresolvable, because omarchy and quickshell install on neither the CI
runner nor a plain checkout.

`tests/qml/run.sh` projects an `events.json` v1 fixture through the real
`list-display-json` and runs the QML tests of `tests/qml/` against that output.
It needs the Qt 6 `qmltestrunner` — `qt6-declarative-dev-tools` on Debian and
Ubuntu, `qt6-declarative` on Arch — and takes a `QMLTESTRUNNER` override.

`tests/widget-harness/run.sh` runs the real `widget/BarWidget.qml` under a nested
Hyprland alongside omarchy's own `Ui` and `Commons`, injects completions through
the binary's hooks, opens the popup over its IPC and captures it with `grim`:

```bash
tests/widget-harness/run.sh --events 7
tests/widget-harness/run.sh --keep   # leave it up to drive by hand
tests/widget-harness/stop.sh
```

The screenshot lands in `target/widget-harness/popup.png`. The harness runs in a
window on the session that starts it, and floats that window at a fixed size so
the session's layout stays put. Keep that window on screen: once the session
stops showing it, the nested compositor stops rendering and the run fails on
the capture.

The running shell is never touched: the state goes to a temporary `XDG_STATE_HOME`,
quickshell is only ever addressed by `--pid`, and the run brings its own
`notify-send`. It needs `foot`, `grim` and `jq`.

## Licence

MIT. See `LICENSE` and `ASSETS.md`.
