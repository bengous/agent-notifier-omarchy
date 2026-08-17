#!/usr/bin/env bash
# One pass over the whole setup: install the binary from the release archive,
# wire the harness hooks through `agent-notifier setup`, print the listener
# block. It sequences; every decision about a config belongs to the binary.
set -euo pipefail

PREFIX="${PREFIX:-${HOME}/.local}"
BINDIR="${PREFIX}/bin"
SHAREDIR="${PREFIX}/share/agent-notifier"
ARCHIVE="${AGENT_NOTIFIER_ONBOARD_ARCHIVE:-}"
REPOSITORY="bengous/agent-notifier-omarchy"
LATEST_RELEASE_URL="https://api.github.com/repos/${REPOSITORY}/releases/latest"
DOWNLOAD_URL="https://github.com/${REPOSITORY}/releases/download"
BIN_NAME="agent-notifier"
SOUND_FILE="agent-complete.mp3"

assume_yes=0
work_dir=""
failed_harnesses=()

usage() {
  cat <<'EOF'
Usage: scripts/onboard.sh [--yes]

  --yes   wire every harness that needs it, with no prompt

Environment:
  PREFIX                            install root (default ~/.local)
  AGENT_NOTIFIER_ONBOARD_ARCHIVE    local release archive, with its .sha256
                                    beside it; skips the download
EOF
}

fail() {
  echo "agent-notifier: $1" >&2
  exit 1
}

note() {
  echo "$1"
}

step() {
  echo
  echo "== $1"
}

cleanup() {
  if [[ -n "${work_dir}" ]]; then
    rm -rf -- "${work_dir}"
  fi
}

package_of() {
  case "$1" in
    sha256sum)
      echo "coreutils"
      ;;
    *)
      echo "$1"
      ;;
  esac
}

preflight() {
  local tool
  local -a missing=()
  for tool in gum curl jq tar sha256sum install; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
      missing+=("$(package_of "${tool}")")
    fi
  done
  if ((${#missing[@]} > 0)); then
    echo "agent-notifier: onboarding needs tools this machine does not have." >&2
    echo "  sudo pacman -S --needed ${missing[*]}" >&2
    exit 1
  fi
}

release_target() {
  local machine
  machine="$(uname -m)"
  case "${machine}" in
    x86_64)
      echo "x86_64-unknown-linux-gnu"
      ;;
    aarch64 | arm64)
      echo "aarch64-unknown-linux-gnu"
      ;;
    *)
      fail "no release archive for ${machine}; build from source instead (README, section Install)"
      ;;
  esac
}

latest_tag() {
  local body tag
  body="$(curl -fsSL "${LATEST_RELEASE_URL}")" ||
    fail "cannot reach the GitHub release API; check the network, or an unauthenticated rate limit of 60 requests an hour"
  tag="$(jq -r '.tag_name // empty' <<<"${body}")"
  [[ -n "${tag}" ]] || fail "the latest release carries no tag"
  echo "${tag}"
}

# Both files land in the work directory under their release names: the checksum
# file names the archive relative to itself.
fetch_release() {
  local target tag name
  target="$(release_target)"
  tag="$(latest_tag)"
  name="${BIN_NAME}-${tag}-${target}.tar.gz"
  note "downloading ${name}"
  curl -fsSL -o "${work_dir}/${name}" "${DOWNLOAD_URL}/${tag}/${name}" ||
    fail "cannot download ${name}"
  curl -fsSL -o "${work_dir}/${name}.sha256" "${DOWNLOAD_URL}/${tag}/${name}.sha256" ||
    fail "cannot download ${name}.sha256"
  echo "${name}"
}

copy_local_archive() {
  local name
  [[ -f "${ARCHIVE}" ]] || fail "${ARCHIVE} is not a file"
  [[ -f "${ARCHIVE}.sha256" ]] || fail "${ARCHIVE}.sha256 is missing"
  name="$(basename -- "${ARCHIVE}")"
  cp -- "${ARCHIVE}" "${work_dir}/${name}"
  cp -- "${ARCHIVE}.sha256" "${work_dir}/${name}.sha256"
  echo "${name}"
}

install_binary() {
  local name binary_tmp
  if [[ -n "${ARCHIVE}" ]]; then
    name="$(copy_local_archive)"
  else
    name="$(fetch_release)"
  fi

  (cd -- "${work_dir}" && sha256sum -c "${name}.sha256" >/dev/null) ||
    fail "the checksum of ${name} does not match"

  mkdir -p -- "${work_dir}/unpacked"
  tar -xzf "${work_dir}/${name}" -C "${work_dir}/unpacked"
  [[ -f "${work_dir}/unpacked/${BIN_NAME}" ]] ||
    fail "the archive carries no ${BIN_NAME}"

  # Data before binary: a half-finished install must never leave a binary
  # looking for a sound file that is not there yet.
  install -d "${SHAREDIR}" "${BINDIR}"
  install -m 644 "${work_dir}/unpacked/data/${SOUND_FILE}" "${SHAREDIR}/${SOUND_FILE}"
  binary_tmp="$(mktemp "${BINDIR}/.${BIN_NAME}.tmp.XXXXXX")"
  install -m 755 "${work_dir}/unpacked/${BIN_NAME}" "${binary_tmp}"
  mv -f -- "${binary_tmp}" "${BINDIR}/${BIN_NAME}"
  note "installed ${BINDIR}/${BIN_NAME} and ${SHAREDIR}/${SOUND_FILE}"
}

step_binary() {
  local installed
  step "1/3  the binary"
  if installed="$(command -v "${BIN_NAME}")"; then
    note "already installed: ${installed}"
    return 0
  fi
  install_binary
  export PATH="${BINDIR}:${PATH}"
  case ":${PATH}:" in
    *":${BINDIR}:"*) ;;
    *)
      note "warning: ${BINDIR} is not on the PATH of your session; the widget calls ${BIN_NAME} through PATH"
      ;;
  esac
}

harnesses_needing_setup() {
  local report
  report="$("${BIN_NAME}" doctor --json)"
  jq -r '.harnesses[]
    | select(.harness == "claude" or .harness == "codex")
    | select(.state != "wired" and .state != "harness-absent")
    | .harness' <<<"${report}"
}

lines_of() {
  local raw="$1"
  local -n target="$2"
  target=()
  if [[ -n "${raw}" ]]; then
    mapfile -t target <<<"${raw}"
  fi
}

chosen_harnesses() {
  local raw
  local -a needed=()
  raw="$(harnesses_needing_setup)"
  lines_of "${raw}" needed

  if ((assume_yes == 1)); then
    printf '%s\n' "${needed[@]}"
    return 0
  fi
  if [[ ! -t 0 ]]; then
    fail "onboarding needs a terminal to ask which harnesses to wire; rerun with --yes"
  fi
  local selected
  selected="$(
    IFS=,
    echo "${needed[*]}"
  )"
  gum choose --no-limit --header "Wire the completion hook of:" \
    --selected "${selected}" claude codex
}

step_hooks() {
  local harness raw
  local -a chosen=()
  step "2/3  the harness hooks"
  "${BIN_NAME}" doctor
  echo
  raw="$(chosen_harnesses)"
  lines_of "${raw}" chosen

  if ((${#chosen[@]} == 0)); then
    note "nothing to wire"
    return 0
  fi
  for harness in "${chosen[@]}"; do
    if ! "${BIN_NAME}" setup "${harness}"; then
      failed_harnesses+=("${harness}")
    fi
  done
}

step_listener() {
  local binary
  binary="$(command -v "${BIN_NAME}")"
  step "3/3  the focused-window listener"
  cat <<EOF
Focusing a window by hand marks its events read. That needs a listener this
script does not start and does not install: add this line to
~/.config/hypr/autostart.lua yourself, then restart your session.

o.exec_on_start("${binary} watch-focused-window")
EOF
}

while (($#)); do
  case "$1" in
    --yes)
      assume_yes=1
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

trap cleanup EXIT
work_dir="$(mktemp -d)"

preflight
step_binary
step_hooks
step_listener

step "the setup now reads"
"${BIN_NAME}" doctor

if ((${#failed_harnesses[@]} > 0)); then
  fail "these harnesses were not wired: ${failed_harnesses[*]}"
fi
