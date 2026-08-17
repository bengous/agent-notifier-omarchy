#!/usr/bin/env bash
# Replays the fixture completions from inside the harness terminal. A hook only
# resolves a source window when the compositor owns one of its ancestors, so
# every fixture has to run here rather than from run.sh.
set -euo pipefail
shopt -s nullglob

RUN_DIR=${1:?usage: inject.sh RUN_DIR}

# run.sh owns the compositor choreography: it maps this terminal, drops its
# focus, and only then releases the fixtures.
until [[ -f ${RUN_DIR}/go ]]; do
  sleep 0.1
done

payloads=("${RUN_DIR}"/fixtures/*.json)
((${#payloads[@]})) || {
  echo "agent-notifier: no fixture payload in ${RUN_DIR}/fixtures" >&2
  exit 1
}

for payload in "${payloads[@]}"; do
  command=$(<"${payload%.json}.cmd")
  agent-notifier "${command}" <"${payload}"
done

touch "${RUN_DIR}/injected"

# Stored events stay listed only while their source window lives, and this
# terminal is that window.
while true; do
  sleep 3600
done
