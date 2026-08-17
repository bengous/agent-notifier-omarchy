#!/usr/bin/env bash
# Runs the real BarWidget under a nested Hyprland, fills it with completions
# captured by the real binary, opens the popup over the widget's own IPC and
# photographs it.

# The harness polls a compositor: the helpers below are predicates whose failure
# is an answer rather than an error, so set -e is off inside them on purpose.
# shellcheck disable=SC2310

set -euo pipefail

REPO_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
HARNESS_DIR="${REPO_DIR}/tests/widget-harness"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
RUN_DIR="${RUNTIME_DIR}/agent-notifier-widget-harness"
OMARCHY_SHELL_DIR=/usr/share/omarchy/shell
HOST_INSTANCE="${HYPRLAND_INSTANCE_SIGNATURE:-}"
NESTED_WIDTH=1280
NESTED_HEIGHT=800
NESTED_MARGIN=60
CAPTURE_TIMEOUT_SECONDS=15
HOST_WINDOW_TIMEOUT_SECONDS=15
INJECTOR_CLASS="agent-notifier-injector"
# An agent with no session title falls back to its window title, so the
# injector's title is what those rows read on the screenshot.
INJECTOR_TITLE="agent-notifier fixtures"
INJECTOR_WORKSPACE=9
FIXTURE_AGE_STEP_SECONDS=1020
MINIMUM_SCREENSHOT_BYTES=1024
# The onboarding scenario runs the real script against a fake home and a local
# release archive: nothing it writes leaves RUN_DIR, and nothing it reads
# leaves this machine.
ONBOARD_HOME="${RUN_DIR}/home"
ONBOARD_BINDIR="${ONBOARD_HOME}/.local/bin"
ONBOARD_ARCHIVE="${RUN_DIR}/agent-notifier-harness.tar.gz"

AGENTS=(claude codex pi)
PROJECTS=(alpha beta)
TITLES=(
  "Rework the event store lock"
  "Wire the popup to list-display-json"
  "Ship the nested compositor harness"
  "Drop the bare-Hyprland fallback"
  "Name the mark-read-on-focus contract"
  "Split the display projection"
)

event_count=5
keep=0
scenario=default
screenshot=""

usage() {
  cat <<'EOF'
Usage: tests/widget-harness/run.sh [--events N] [--out FILE] [--keep] [--scenario NAME]

  --events N        completions to inject through the real binary (default 5)
  --out FILE        screenshot path (default target/widget-harness/<scenario>.png)
  --keep            leave the harness running instead of stopping it
  --scenario NAME   default; binary-missing: no binary on PATH, setup card
                    shown, Configure CTA logged, recovery after the binary
                    returns; onboarding: the same start, then the real
                    scripts/onboard.sh installs and wires everything
EOF
}

fail() {
  echo "agent-notifier: $1" >&2
  exit 1
}

while (($#)); do
  case $1 in
    --events)
      (($# >= 2)) || fail "$1 needs a value"
      event_count=$2
      shift 2
      ;;
    --out)
      (($# >= 2)) || fail "$1 needs a value"
      screenshot=$2
      shift 2
      ;;
    --keep)
      keep=1
      shift
      ;;
    --scenario)
      (($# >= 2)) || fail "$1 needs a value"
      scenario=$2
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "agent-notifier: unknown argument $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

on_exit() {
  local status=$? artifacts
  ((status == 0)) && return 0
  artifacts="$(dirname -- "${screenshot}")"
  mkdir -p -- "${artifacts}"
  hyprctl -j clients >"${artifacts}/clients.json" 2>/dev/null || true
  cp -- "${RUN_DIR}"/*.log "${artifacts}/" 2>/dev/null && echo "agent-notifier: logs copied to ${artifacts}" >&2
  "${HARNESS_DIR}/stop.sh" >/dev/null 2>&1 || true
  return "${status}"
}
trap on_exit EXIT

case ${scenario} in
  default | binary-missing | onboarding) ;;
  *) fail "unknown scenario ${scenario}" ;;
esac

# Codex keeps its config and its sessions in one directory, and the onboarding
# scenario wants both inside its fake home: that is where setup writes.
codex_dir="${RUN_DIR}/codex"
if [[ ${scenario} == onboarding ]]; then
  codex_dir="${ONBOARD_HOME}/.codex"
fi
if [[ -z ${screenshot} ]]; then
  if [[ ${scenario} == default ]]; then
    screenshot="${REPO_DIR}/target/widget-harness/popup.png"
  else
    screenshot="${REPO_DIR}/target/widget-harness/${scenario}.png"
  fi
fi

for tool in Hyprland qs grim jq foot cargo; do
  command -v "${tool}" >/dev/null || fail "the widget harness needs ${tool} on PATH"
done
[[ -d ${OMARCHY_SHELL_DIR} ]] || fail "${OMARCHY_SHELL_DIR} is missing; the harness reuses omarchy's Ui and Commons"
if [[ ! ${event_count} =~ ^[1-9][0-9]*$ ]]; then
  fail "--events takes a positive number"
fi

wait_for() {
  local what=$1 timeout=$2 deadline
  shift 2
  deadline=$((SECONDS + timeout))
  until "$@"; do
    ((SECONDS < deadline)) || fail "timed out after ${timeout}s waiting for ${what}"
    sleep 0.1
  done
}

"${HARNESS_DIR}/stop.sh" >/dev/null
mkdir -p "${RUN_DIR}"/{bin,state,fixtures,projects,transcripts,shell}
mkdir -p -- "${codex_dir}/sessions"
mkdir -p -- "$(dirname -- "${screenshot}")"

cargo build --quiet --manifest-path "${REPO_DIR}/Cargo.toml"
# Every scenario but the default starts with no binary anywhere on the widget's
# PATH; binary-missing restores this symlink later to observe the recovery, and
# onboarding lets the real script install one.
if [[ ${scenario} == default ]]; then
  ln -s "${REPO_DIR}/target/debug/agent-notifier" "${RUN_DIR}/bin/agent-notifier"
fi
# The hook always alerts. A harness run must not reach the user's notification
# daemon, so its own notify-send shadows the real one.
printf '#!/bin/sh\nexit 0\n' >"${RUN_DIR}/bin/notify-send"
chmod +x "${RUN_DIR}/bin/notify-send"
# The widget's CTA runs the onboarding script through this helper; the harness
# shadows it with a logger so a run never opens a terminal on the host session.
cat >"${RUN_DIR}/bin/omarchy-launch-floating-terminal-with-presentation" <<EOF
#!/bin/sh
echo "\$*" >>"${RUN_DIR}/launch.log"
EOF
chmod +x "${RUN_DIR}/bin/omarchy-launch-floating-terminal-with-presentation"

# quickshell resolves qs.<dir> against the config root, so omarchy's own Ui and
# Commons are all the real widget needs to run outside omarchy-shell.
ln -s "${OMARCHY_SHELL_DIR}/Ui" "${RUN_DIR}/shell/Ui"
ln -s "${OMARCHY_SHELL_DIR}/Commons" "${RUN_DIR}/shell/Commons"
# The link is the plugin root, never widget/: the components read assets/ one
# level above themselves, and Qt resolves that against the linked path.
ln -s "${REPO_DIR}" "${RUN_DIR}/shell/AgentNotifier"
cp "${HARNESS_DIR}/shell.qml" "${RUN_DIR}/shell/shell.qml"

seed_projects() {
  local project
  for project in "${PROJECTS[@]}"; do
    git init --quiet -b main "${RUN_DIR}/projects/${project}"
  done
}

# The onboarding scenario needs a machine that looks bare: an empty home with
# the two harness directories, a stub for each harness so doctor diagnoses it
# at all, and a local release archive in the layout the real one ships.
seed_onboarding() {
  local harness archive_name
  mkdir -p "${ONBOARD_HOME}/.claude" "${RUN_DIR}/pack/data"
  for harness in claude codex; do
    printf '#!/bin/sh\nexit 0\n' >"${RUN_DIR}/bin/${harness}"
    chmod +x "${RUN_DIR}/bin/${harness}"
  done
  cp "${REPO_DIR}/target/debug/agent-notifier" "${RUN_DIR}/pack/agent-notifier"
  cp "${REPO_DIR}/data/agent-complete.mp3" "${RUN_DIR}/pack/data/agent-complete.mp3"
  tar -czf "${ONBOARD_ARCHIVE}" -C "${RUN_DIR}/pack" agent-notifier data
  archive_name="$(basename -- "${ONBOARD_ARCHIVE}")"
  (cd -- "${RUN_DIR}" && sha256sum "${archive_name}" >"${ONBOARD_ARCHIVE}.sha256")
}

if [[ ${scenario} == default ]]; then
  seed_projects
elif [[ ${scenario} == onboarding ]]; then
  seed_onboarding
fi

write_fixtures() {
  local index agent project session title payload
  for ((index = 0; index < event_count; index++)); do
    agent=${AGENTS[index % ${#AGENTS[@]}]}
    project="${RUN_DIR}/projects/${PROJECTS[index % ${#PROJECTS[@]}]}"
    session="harness-${index}"
    title="${TITLES[index % ${#TITLES[@]}]}"
    payload="${RUN_DIR}/fixtures/$(printf '%02d' "${index}")"
    case ${agent} in
      claude)
        jq -nc --arg id "${session}" --arg title "${title}" \
          '{type:"ai-title", sessionId:$id, aiTitle:$title}' \
          >"${RUN_DIR}/transcripts/${session}.jsonl"
        jq -nc --arg cwd "${project}" --arg id "${session}" \
          --arg transcript "${RUN_DIR}/transcripts/${session}.jsonl" \
          '{cwd:$cwd, session_id:$id, transcript_path:$transcript}' >"${payload}.json"
        echo claude-hook >"${payload}.cmd"
        ;;
      codex)
        jq -nc --arg title "${title}" \
          '{type:"event_msg", payload:{type:"user_message", message:$title}}' \
          >"${codex_dir}/sessions/rollout-${session}.jsonl"
        jq -nc --arg cwd "${project}" --arg id "${session}" \
          '{cwd:$cwd, session_id:$id}' >"${payload}.json"
        echo hook >"${payload}.cmd"
        ;;
      pi)
        jq -nc --arg cwd "${project}" --arg id "${session}" \
          '{cwd:$cwd, session_id:$id}' >"${payload}.json"
        echo pi-hook >"${payload}.cmd"
        ;;
      *)
        fail "no fixture recipe for agent ${agent}"
        ;;
    esac
  done
}
if [[ ${scenario} == default ]]; then
  write_fixtures
fi

shopt -s nullglob

host_display="${WAYLAND_DISPLAY:-wayland-1}"
existing_instances=("${RUNTIME_DIR}"/hypr/*/)

env -u HYPRLAND_INSTANCE_SIGNATURE WAYLAND_DISPLAY="${host_display}" \
  AQ_DRM_DEVICES=/dev/null \
  HYPRLAND_NO_SD_NOTIFY=1 HYPRLAND_NO_SD_VARS=1 HYPRLAND_NO_CRASHREPORTER=1 \
  Hyprland -c "${HARNESS_DIR}/hypr-test.conf" >"${RUN_DIR}/hyprland.log" 2>&1 &
hyprland_pid=$!
echo "${hyprland_pid}" >"${RUN_DIR}/hyprland.pid"

find_instance() {
  local path known
  for path in "${RUNTIME_DIR}"/hypr/*/; do
    for known in "${existing_instances[@]}"; do
      [[ ${path} == "${known}" ]] && continue 2
    done
    [[ -S ${path}.socket.sock ]] || continue
    basename -- "${path}" >"${RUN_DIR}/instance"
    return 0
  done
  return 1
}
wait_for "the nested compositor" 30 find_instance

# The nested compositor is a window on the session that started it. Tiled, it
# reflows the user's layout on open and again on close; floating it at a fixed
# size leaves the session alone and gives every run the same frame. The only
# window the harness ever touches out there is the one it just opened.
host_window() {
  local clients
  clients=$(env HYPRLAND_INSTANCE_SIGNATURE="${HOST_INSTANCE}" hyprctl -j clients)
  jq -e --argjson pid "${hyprland_pid}" 'map(select(.pid == $pid)) | .[0]' <<<"${clients}"
}

# The host is Omarchy's Hyprland: its `hyprctl dispatch` is a Lua shorthand
# for hl.dispatch(...), so classic dispatcher syntax fails with a parse error.
float_host_window() {
  local window address
  window=$(host_window) || return 1
  address=$(jq -r '.address' <<<"${window}")
  for command in \
    "hl.dsp.window.float({ action = \"on\", window = \"address:${address}\" })" \
    "hl.dsp.window.resize({ x = ${NESTED_WIDTH}, y = ${NESTED_HEIGHT}, window = \"address:${address}\" })" \
    "hl.dsp.window.move({ x = ${NESTED_MARGIN}, y = ${NESTED_MARGIN}, window = \"address:${address}\" })"; do
    env HYPRLAND_INSTANCE_SIGNATURE="${HOST_INSTANCE}" \
      hyprctl dispatch "${command}" >/dev/null
  done
  window=$(host_window) || return 1
  jq -e --argjson width "${NESTED_WIDTH}" --argjson height "${NESTED_HEIGHT}" \
    '.floating and .size[0] == $width and .size[1] == $height' <<<"${window}" >/dev/null
}

# A resize aimed at a window the host still tiles does nothing, so the three
# calls are re-asserted until the host reports the shape back. Losing that race
# costs the fixed frame size, never the run.
settle_host_window() {
  local deadline=$((SECONDS + HOST_WINDOW_TIMEOUT_SECONDS))
  while ((SECONDS < deadline)); do
    float_host_window && return 0
    sleep 0.2
  done
  return 1
}

if [[ -n ${HOST_INSTANCE} ]]; then
  settle_host_window ||
    echo "agent-notifier: the harness window never reported ${NESTED_WIDTH}x${NESTED_HEIGHT} floating; capturing the shape the host gave it" >&2
fi

HYPRLAND_INSTANCE_SIGNATURE="$(<"${RUN_DIR}/instance")"
export HYPRLAND_INSTANCE_SIGNATURE

# The nested compositor picks its own Wayland socket and only announces it here.
read_nested_display() {
  nested_display=$(grep -o 'WAYLAND_DISPLAY: wayland-[0-9]*' "${RUN_DIR}/hyprland.log" |
    tail -1 | cut -d' ' -f2)
  [[ -n ${nested_display} ]]
}
wait_for "the nested Wayland socket" 30 read_nested_display

monitor_has_mode() { hyprctl -j monitors 2>/dev/null | jq -e '.[0].width > 0' >/dev/null; }
wait_for "the nested monitor mode" 30 monitor_has_mode
# The requested mode is only a request: read back what the compositor settled on.
monitor=$(hyprctl -j monitors | jq -r '.[0].name')
monitor_size=$(hyprctl -j monitors | jq -r '"\(.[0].width)x\(.[0].height)"')

# The dev machine usually carries agent-notifier on PATH. The binary-missing
# scenario needs the widget's lookup to fail, so every directory holding the
# binary is dropped from the PATH the harness hands to quickshell.
harness_path="${RUN_DIR}/bin:${PATH}"
if [[ ${scenario} != default ]]; then
  harness_path="${RUN_DIR}/bin"
  # The onboarding install target leads: it holds nothing yet, so the widget
  # still starts on the missing-binary face.
  if [[ ${scenario} == onboarding ]]; then
    harness_path="${ONBOARD_BINDIR}:${harness_path}"
  fi
  IFS=: read -ra path_dirs <<<"${PATH}"
  for dir in "${path_dirs[@]}"; do
    if [[ -n ${dir} && ! -x ${dir}/agent-notifier ]]; then
      harness_path+=":${dir}"
    fi
  done
fi

harness_env=(
  WAYLAND_DISPLAY="${nested_display}"
  HYPRLAND_INSTANCE_SIGNATURE="${HYPRLAND_INSTANCE_SIGNATURE}"
  PATH="${harness_path}"
  XDG_STATE_HOME="${RUN_DIR}/state"
  CODEX_HOME="${codex_dir}"
  AGENT_NOTIFIER_SOUND=0
)
# Only this scenario moves HOME: the hook configs it writes must be the ones
# the widget's own probe reads back. The XDG directories stay real so omarchy's
# Ui and Commons keep finding the theme they render with.
if [[ ${scenario} == onboarding ]]; then
  harness_env+=(
    HOME="${ONBOARD_HOME}"
    XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
    XDG_DATA_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}"
    XDG_CACHE_HOME="${XDG_CACHE_HOME:-${HOME}/.cache}"
    PREFIX="${ONBOARD_HOME}/.local"
    AGENT_NOTIFIER_ONBOARD_ARCHIVE="${ONBOARD_ARCHIVE}"
  )
fi

inject_fixtures() {
  local stored state_file
  env "${harness_env[@]}" foot --app-id="${INJECTOR_CLASS}" --title="${INJECTOR_TITLE}" \
    bash "${HARNESS_DIR}/inject.sh" "${RUN_DIR}" >"${RUN_DIR}/injector.log" 2>&1 &
  echo $! >"${RUN_DIR}/injector.pid"

  injector_address() {
    hyprctl -j clients |
      jq -er --arg class "${INJECTOR_CLASS}" 'map(select(.class == $class)) | .[0].address'
  }
  injector_is_mapped() { injector_address >/dev/null; }
  wait_for "the injector window" 30 injector_is_mapped

  # A hook whose source window holds the focus discards its own completion, and
  # list-display-json marks that window's events read. Parking the injector out
  # of focus is what lets the fixtures reach the popup — and keeps it out of
  # frame. The terminal can ask for activation again after it maps, so the move
  # is re-asserted until the compositor reports no focused window at all.
  park_injector() {
    local address focused
    address=$(injector_address)
    hyprctl dispatch movetoworkspacesilent "${INJECTOR_WORKSPACE},address:${address}" >/dev/null
    focused=$(hyprctl -j activewindow | jq -r '.address // ""')
    [[ -z ${focused} ]]
  }
  wait_for "the injector window to leave the focus" 30 park_injector

  touch "${RUN_DIR}/go"
  injection_is_done() { [[ -f ${RUN_DIR}/injected ]]; }
  wait_for "the fixture completions" 60 injection_is_done

  stored=$(env "${harness_env[@]}" agent-notifier list-json | jq '.events | length')
  [[ ${stored} =~ ^[0-9]+$ ]] ||
    fail "list-json returned no count; see ${RUN_DIR}/injector.log"
  ((stored == event_count)) ||
    fail "the binary stored ${stored} of ${event_count} completions; see ${RUN_DIR}/injector.log"

  # Every fixture is captured within the same second, which would render as one
  # wall of "just now". Ageing them apart is what puts each relative-time branch
  # on the screenshot.
  state_file="${RUN_DIR}/state/agent-notifier/events.json"
  jq --argjson step "${FIXTURE_AGE_STEP_SECONDS}" \
    '.events |= (to_entries | map(.value.createdAt =
      ((.value.createdAt | sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601) - .key * $step
       | todateiso8601)) | map(.value))' "${state_file}" >"${state_file}.aged"
  mv "${state_file}.aged" "${state_file}"
}

if [[ ${scenario} == default ]]; then
  inject_fixtures
fi

# Without an explicit wayland platform Qt falls back to xcb: the shell loads and
# nothing ever renders.
env "${harness_env[@]}" QT_QPA_PLATFORM=wayland \
  qs -p "${RUN_DIR}/shell/shell.qml" >"${RUN_DIR}/quickshell.log" 2>&1 &
quickshell_pid=$!
echo "${quickshell_pid}" >"${RUN_DIR}/quickshell.pid"

# --pid, never the config name: the user's own shell must stay untouched.
# Until the shell answers, the probe is not JSON — every reader of it polls.
probe() { qs ipc --pid "${quickshell_pid}" call harness probe 2>/dev/null; }
probe_says() { probe | jq -e "$1" >/dev/null 2>&1; }
widget_is_filled() { probe_says ".events == ${event_count}"; }
cli_reported_missing() { probe_says '.cliMissing and .face == "binary-missing"'; }
if [[ ${scenario} == default ]]; then
  wait_for "the widget to list the completions" 30 widget_is_filled
else
  wait_for "the missing-binary state" 30 cli_reported_missing
fi

qs ipc --pid "${quickshell_pid}" call io.github.bengous.agent-notifier open >/dev/null
popup_is_open() { probe_says '.popupOpen'; }
wait_for "the popup to open" 30 popup_is_open

card_is_visible() { probe_says '.cardVisible'; }
if [[ ${scenario} != default ]]; then
  wait_for "the setup card" 10 card_is_visible
fi

# The whole marketplace journey, in the order a user lives it: the card is up,
# the CTA hands one script to the terminal helper, that script installs and
# wires everything, and the widget notices on its own.
run_onboarding() {
  qs ipc --pid "${quickshell_pid}" call harness launchOnboarding >/dev/null
  onboarding_is_logged() { grep -q "scripts/onboard.sh" "${RUN_DIR}/launch.log" 2>/dev/null; }
  wait_for "the onboarding launch log" 10 onboarding_is_logged

  # The shadowed helper only logs the launch, so the run executes the script
  # itself — same command, same environment as the widget would give it.
  env "${harness_env[@]}" bash "${REPO_DIR}/scripts/onboard.sh" --yes \
    >"${RUN_DIR}/onboard.log" 2>&1 ||
    fail "onboard.sh failed; see ${RUN_DIR}/onboard.log"

  [[ -x ${ONBOARD_BINDIR}/agent-notifier ]] ||
    fail "onboard.sh installed no binary in ${ONBOARD_BINDIR}"
  grep -q "agent-notifier claude-hook" "${ONBOARD_HOME}/.claude/settings.json" ||
    fail "the Claude hook is missing from the onboarded settings"
  grep -q "agent-notifier hook" "${codex_dir}/config.toml" ||
    fail "the Codex hook is missing from the onboarded config"

  cli_recovered() { probe_says '.cliMissing | not'; }
  wait_for "the binary recovery" 30 cli_recovered
  setup_is_ready() { probe_says '.setupReady'; }
  wait_for "the setup to read ready" 30 setup_is_ready

  # A wired machine ends where the widget's own job starts: one completion,
  # captured by the binary this journey installed.
  seed_projects
  write_fixtures
  inject_fixtures
  qs ipc --pid "${quickshell_pid}" call io.github.bengous.agent-notifier close >/dev/null
  qs ipc --pid "${quickshell_pid}" call io.github.bengous.agent-notifier open >/dev/null
  wait_for "the popup to reopen" 30 popup_is_open
  wait_for "the widget to list the completions" 30 widget_is_filled
}

if [[ ${scenario} == onboarding ]]; then
  run_onboarding
fi

# The compositor greets every run with its own deprecation notices, and they
# would land on the screenshot.
hyprctl dismissnotify >/dev/null

# PopupCard fades the card in over 140ms and the animation has no observable
# end: nothing here polls, the wait is the animation itself.
sleep 0.5

# A harness window the host stops showing takes the nested compositor's frame
# callbacks with it, and grim then waits for a frame that never comes. The host
# reporting `visible: false` is what predicts it; a fullscreen window elsewhere
# on the session does not.
capture_hint() {
  local window
  window=$(host_window 2>/dev/null) || return 0
  if jq -e '.visible | not' <<<"${window}" >/dev/null 2>&1; then
    echo "; the host reports the harness window as not visible, so the nested compositor stopped rendering"
  fi
  return 0
}

# grim rejects -o together with -g.
if ! timeout "${CAPTURE_TIMEOUT_SECONDS}" \
  env WAYLAND_DISPLAY="${nested_display}" grim -o "${monitor}" "${screenshot}"; then
  hint=$(capture_hint)
  fail "no frame from ${monitor} in ${CAPTURE_TIMEOUT_SECONDS}s${hint}"
fi
[[ -s ${screenshot} ]] || fail "grim wrote no screenshot"
screenshot_bytes=$(stat -c %s -- "${screenshot}")
((screenshot_bytes > MINIMUM_SCREENSHOT_BYTES)) || fail "the screenshot is empty"

if [[ ${scenario} == binary-missing ]]; then
  # The verb reaches launchOnboarding() directly; the shadowed helper turns the
  # detached launch into one loggable line.
  qs ipc --pid "${quickshell_pid}" call harness launchOnboarding >/dev/null
  launch_is_logged() { grep -q "scripts/onboard.sh" "${RUN_DIR}/launch.log" 2>/dev/null; }
  wait_for "the onboarding launch log" 10 launch_is_logged

  # Restoring the binary must recover the widget within one re-probe window.
  ln -s "${REPO_DIR}/target/debug/agent-notifier" "${RUN_DIR}/bin/agent-notifier"
  cli_recovered() { probe_says '.cliMissing | not'; }
  wait_for "the binary recovery" 30 cli_recovered

  # The reverse degradation: a binary that disappears after a successful run
  # must fail the next refresh, not coast on the last exit it saw.
  rm "${RUN_DIR}/bin/agent-notifier"
  qs ipc --pid "${quickshell_pid}" call io.github.bengous.agent-notifier close >/dev/null
  qs ipc --pid "${quickshell_pid}" call io.github.bengous.agent-notifier open >/dev/null
  wait_for "the degradation back to binary-missing" 10 cli_reported_missing

  echo "harness: binary-missing journey replayed: card shown, Configure CTA logged, recovery and degradation observed"
elif [[ ${scenario} == onboarding ]]; then
  echo "harness: onboarding journey replayed: card shown, Configure CTA logged, binary installed in ${ONBOARD_BINDIR}, claude and codex wired, ${event_count} completions listed"
else
  echo "harness: ${event_count} completions injected, popup captured on ${monitor} (${monitor_size})"
fi
echo "harness: screenshot ${screenshot}"

if ((keep == 0)); then
  "${HARNESS_DIR}/stop.sh"
else
  cat <<EOF
harness: still running.
  probe    qs ipc --pid ${quickshell_pid} call harness probe
  popup    qs ipc --pid ${quickshell_pid} call io.github.bengous.agent-notifier toggle
  capture  WAYLAND_DISPLAY=${nested_display} grim -o ${monitor} <file>
  stop     ${HARNESS_DIR}/stop.sh
EOF
fi
