#!/usr/bin/env bash
# The deterministic widget gates: the real binary projects a v1 state fixture
# through list-display-json, and the QML contract runs against that output.
set -euo pipefail

REPO_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
QML_DIR="${REPO_DIR}/tests/qml"
WORK_DIR="${QML_DIR}/generated"

fail() {
  echo "agent-notifier: $1" >&2
  exit 1
}

# Never the bare name on PATH: on Arch that is Qt 5's runner, which loads no
# Qt 6 module and exits non-zero without printing a word.
runner="${QMLTESTRUNNER:-/usr/lib/qt6/bin/qmltestrunner}"
[[ -x ${runner} ]] || runner="$(command -v qmltestrunner6 || true)"
[[ -x ${runner} ]] || fail "the Qt 6 qmltestrunner is missing; install qt6-declarative-dev-tools"

# proc(5) numbers starttime 22, and the fields split only past the last ')'.
process_start_time() {
  local stat fields
  stat=$(</proc/"$1"/stat)
  read -ra fields <<<"${stat##*') '}"
  echo "${fields[19]}"
}

cargo build --quiet --manifest-path "${REPO_DIR}/Cargo.toml"

rm -rf -- "${WORK_DIR}"
mkdir -p "${WORK_DIR}/state/agent-notifier"

# A completion is listed only while its source process lives, so the fixture
# windows borrow this shell.
jq --argjson pid "$$" --argjson start "$(process_start_time $$)" \
  '.events |= map(.workspace.sourceProcess = {pid: $pid, startTime: $start})' \
  "${QML_DIR}/fixtures/events-v1.json" >"${WORK_DIR}/state/agent-notifier/events.json"

XDG_STATE_HOME="${WORK_DIR}/state" "${REPO_DIR}/target/debug/agent-notifier" list-display-json \
  >"${WORK_DIR}/display-state.json"

# offscreen is the platform Qt Quick Test documents for headless runs, and Qt
# refuses to read a local file over XMLHttpRequest until it is told to.
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QML_XHR_ALLOW_FILE_READ=1 \
  "${runner}" -input "${QML_DIR}"
