#!/usr/bin/env bash
# Runs the real BarWidget under a nested Hyprland, fills it with completions
# captured by the real binary, opens the popup over the widget's own IPC and
# photographs it.
set -euo pipefail

REPO_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
HARNESS_DIR="${REPO_DIR}/tests/widget-harness"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
RUN_DIR="${RUNTIME_DIR}/agent-notifier-widget-harness"
OMARCHY_SHELL_DIR=/usr/share/omarchy/shell
INJECTOR_CLASS="agent-notifier-injector"
# An agent with no session title falls back to its window title, so the
# injector's title is what those rows read on the screenshot.
INJECTOR_TITLE="agent-notifier fixtures"
INJECTOR_WORKSPACE=9
FIXTURE_AGE_STEP_SECONDS=1020
MINIMUM_SCREENSHOT_BYTES=1024

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
screenshot="${REPO_DIR}/target/widget-harness/popup.png"

usage() {
  cat <<'EOF'
Usage: tests/widget-harness/run.sh [--events N] [--out FILE] [--keep]

  --events N   completions to inject through the real binary (default 5)
  --out FILE   screenshot path (default target/widget-harness/popup.png)
  --keep       leave the harness running instead of stopping it
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
mkdir -p "${RUN_DIR}"/{bin,state,fixtures,projects,transcripts,codex/sessions,shell}
mkdir -p -- "$(dirname -- "${screenshot}")"

cargo build --quiet --manifest-path "${REPO_DIR}/Cargo.toml"
ln -s "${REPO_DIR}/target/debug/agent-notifier" "${RUN_DIR}/bin/agent-notifier"
# The hook always alerts. A harness run must not reach the user's notification
# daemon, so its own notify-send shadows the real one.
printf '#!/bin/sh\nexit 0\n' >"${RUN_DIR}/bin/notify-send"
chmod +x "${RUN_DIR}/bin/notify-send"

# quickshell resolves qs.<dir> against the config root, so omarchy's own Ui and
# Commons are all the real widget needs to run outside omarchy-shell.
ln -s "${OMARCHY_SHELL_DIR}/Ui" "${RUN_DIR}/shell/Ui"
ln -s "${OMARCHY_SHELL_DIR}/Commons" "${RUN_DIR}/shell/Commons"
ln -s "${REPO_DIR}" "${RUN_DIR}/shell/AgentNotifier"
cp "${HARNESS_DIR}/shell.qml" "${RUN_DIR}/shell/shell.qml"

for project in "${PROJECTS[@]}"; do
  git init --quiet -b main "${RUN_DIR}/projects/${project}"
done

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
          >"${RUN_DIR}/codex/sessions/rollout-${session}.jsonl"
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
write_fixtures

shopt -s nullglob

host_display="${WAYLAND_DISPLAY:-wayland-1}"
existing_instances=("${RUNTIME_DIR}"/hypr/*/)

env -u HYPRLAND_INSTANCE_SIGNATURE WAYLAND_DISPLAY="${host_display}" \
  AQ_DRM_DEVICES=/dev/null \
  HYPRLAND_NO_SD_NOTIFY=1 HYPRLAND_NO_SD_VARS=1 HYPRLAND_NO_CRASHREPORTER=1 \
  Hyprland -c "${HARNESS_DIR}/hypr-test.conf" >"${RUN_DIR}/hyprland.log" 2>&1 &
echo $! >"${RUN_DIR}/hyprland.pid"

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

harness_env=(
  WAYLAND_DISPLAY="${nested_display}"
  HYPRLAND_INSTANCE_SIGNATURE="${HYPRLAND_INSTANCE_SIGNATURE}"
  PATH="${RUN_DIR}/bin:${PATH}"
  XDG_STATE_HOME="${RUN_DIR}/state"
  CODEX_HOME="${RUN_DIR}/codex"
  AGENT_NOTIFIER_SOUND=0
)

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
# list-display-json marks that window's events read. Parking the injector out of
# focus is what lets the fixtures reach the popup — and keeps it out of frame.
# The terminal can ask for activation again after it maps, so the move is
# re-asserted until the compositor reports no focused window at all.
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
wait_for "the widget to list the completions" 30 widget_is_filled

qs ipc --pid "${quickshell_pid}" call io.github.bengous.agent-notifier open >/dev/null
popup_is_open() { probe_says '.popupOpen'; }
wait_for "the popup to open" 30 popup_is_open

# The compositor greets every run with its own deprecation notices, and they
# would land on the screenshot.
hyprctl dismissnotify >/dev/null

# PopupCard fades the card in over 140ms and the animation has no observable
# end: nothing here polls, the wait is the animation itself.
sleep 0.5

# grim rejects -o together with -g.
env WAYLAND_DISPLAY="${nested_display}" grim -o "${monitor}" "${screenshot}"
[[ -s ${screenshot} ]] || fail "grim wrote no screenshot"
screenshot_bytes=$(stat -c %s -- "${screenshot}")
((screenshot_bytes > MINIMUM_SCREENSHOT_BYTES)) || fail "the screenshot is empty"

echo "harness: ${event_count} completions injected, popup captured on ${monitor} (${monitor_size})"
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
