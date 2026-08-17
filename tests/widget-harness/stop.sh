#!/usr/bin/env bash
# Stops the widget harness: only the processes it started and only the runtime
# directories it created.
set -euo pipefail

RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
RUN_DIR="${RUNTIME_DIR}/agent-notifier-widget-harness"

if [[ ! -d ${RUN_DIR} ]]; then
  echo "agent-notifier: no widget harness is running"
  exit 0
fi

stop_process() {
  local name=$1 file=${RUN_DIR}/$1.pid pid deadline
  [[ -f ${file} ]] || return 0
  pid=$(<"${file}")
  kill "${pid}" 2>/dev/null || true
  deadline=$((SECONDS + 5))
  while kill -0 "${pid}" 2>/dev/null; do
    if ((SECONDS >= deadline)); then
      kill -9 "${pid}" 2>/dev/null || true
      break
    fi
    sleep 0.1
  done
  echo "stopped ${name} (pid ${pid})"
}

drop_quickshell_runtime() {
  local file=${RUN_DIR}/quickshell.pid link target
  [[ -f ${file} ]] || return 0
  link=${RUNTIME_DIR}/quickshell/by-pid/$(<"${file}")
  target=$(readlink "${link}" 2>/dev/null || true)
  [[ -n ${target} ]] && rm -rf -- "${target}"
  rm -f -- "${link}"
}

stop_process quickshell
stop_process injector
stop_process hyprland
drop_quickshell_runtime

if [[ -f ${RUN_DIR}/instance ]]; then
  rm -rf -- "${RUNTIME_DIR}/hypr/$(<"${RUN_DIR}/instance")"
fi

rm -rf -- "${RUN_DIR}"
echo "widget harness stopped"
