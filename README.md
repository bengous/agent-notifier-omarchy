# Agent Notifier

`agent-notifier` captures Codex, Claude Code, and Pi completion events, alerts
the desktop, exposes unread sessions to Waybar, and opens a GTK completion
center that can focus the source window.

**Linux + Hyprland only**

## Dependencies

| Dependency | Requirement | Behaviour when missing |
|---|---|---|
| Linux, Hyprland, `hyprctl` | Required | Use outside Hyprland is unsupported; hooks alert without storing when no source client resolves, focus/read/display commands remain fail-soft, Waybar remains safe, and `prune-stale` aborts without changing state |
| gjs + GTK 4 | Required for `center` only | `center` fails with a clear message; every other command works |
| Waybar | Optional | The JSON output and the `RTMIN+11` refresh signal are simply unused |
| `notify-send` | Optional | No desktop pop-up; the event is still captured |
| `mpv`, `canberra-gtk-play` | Optional | No sound; both are tried in that order |
| `git` | Optional | Branch metadata is omitted; the cwd is used as the project path |
| Rust ≥ 1.89, Bash | Build only | — |

## Install

From the crate root:

```sh
./install.sh
```

The installer accepts `PREFIX`, `DESTDIR`, `BINDIR`, and `BIN_NAME`. It defaults
to `/usr/local/bin/agent-notifier` and
`/usr/local/share/agent-notifier`.

## Hook wiring

### Codex

The managed source is
`dot_codex/private_config.toml.tmpl`:

```toml
[[hooks.Stop]]

[[hooks.Stop.hooks]]
command = "/home/you/.local/bin/agent-notifier hook"
statusMessage = "Recording Codex completion"
timeout = 5
type = "command"
```

Replace `/home/you` with the user's home directory.

### Claude Code

Merge this entry from `dot_claude/settings.json` into `hooks`:

```json
{
  "Stop": [
    {
      "hooks": [
        {
          "type": "command",
          "command": "~/.local/bin/agent-notifier claude-hook",
          "timeout": 5
        }
      ]
    }
  ]
}
```

### Pi

Pi uses the managed extension at
`dot_pi/agent/extensions/agent-notifier.ts`. It listens only to main-agent
`agent_end` events:

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

`notifyAgentCompletion` sends the payload to
`~/.local/bin/agent-notifier pi-hook`, with a five-second timeout. Chezmoi
deploys the complete extension to
`~/.pi/agent/extensions/agent-notifier.ts`.

## Waybar

The current `dot_config/waybar/config.jsonc` module is:

```jsonc
"custom/agent-notifier": {
  // Persistent launcher for the agent completion center. The pop-up desktop
  // notification remains separate; this button only opens/refocuses the list.
  "format": "{}",
  "exec": "$HOME/.local/bin/agent-notifier waybar",
  "return-type": "json",
  "interval": 30,
  "signal": 11,
  "on-click": "$HOME/.local/bin/agent-notifier center"
},
```

Add `"custom/agent-notifier"` to the desired Waybar module list. Signal 11
matches the notifier's `RTMIN+11` refresh.

## State file

State is stored at:

```text
$XDG_STATE_HOME/agent-notifier/events.json
```

When `XDG_STATE_HOME` is empty or unset, the fallback is:

```text
$HOME/.local/state/agent-notifier/events.json
```

The notifier fails instead of writing to the current directory when neither
location is available.

## Failure policy

| Command | Failure behaviour |
|---|---|
| `hook`, `pi-hook` | Print the error and exit 0 so the agent harness continues |
| `claude-hook` | Print the error and exit 1; Claude Code treats exit 1 as non-blocking |
| `waybar` | Print `{"text":"agents !","tooltip":"Agent notifier unavailable","class":"error"}` and exit 0 |
| `center` | Print launch failures and exit 1; errors from the spawned GJS process remain visible on stderr |
| `list-json` | Print state-path/read failures and exit 1 |
| `list-display-json` | Print state failures and exit 1; missing Hyprland client data produces an empty focusable list |
| `focus-latest` | Treat no focusable event or a failed focus as a successful no-op; state failures exit 1 |
| `focus-id <event-id>` | Print a focus failure and exit 1; a missing id exits 2 |
| `mark-read <event-id>` | State failures exit 1; a missing id exits 2 |
| `active-window-read` | Treat unavailable active-window data as a successful no-op; state failures exit 1 |
| `watch-active-window` | Exit 1 when no Hyprland event socket can be resolved; reconnect indefinitely after runtime disconnects |
| `clear-read`, `clear-all` | State failures exit 1 |
| `prune-stale` | Exit 1 without changing state when `hyprctl clients -j` fails or returns invalid JSON; a successful empty list prunes every event |
| `--help`, `--version` | Print output and exit 0 |
| Unknown commands, extra arguments | Print help and exit 2 |
