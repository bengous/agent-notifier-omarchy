#!/usr/bin/env bash
set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
DESTDIR="${DESTDIR:-}"
BINDIR="${BINDIR:-${PREFIX}/bin}"
BIN_NAME="${BIN_NAME:-agent-notifier}"
SHAREDIR="${PREFIX}/share/agent-notifier"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${here}"

cargo build --release --locked

# Data first, binary last: a half-finished install must never leave a newer binary
# looking for data that is not there yet.
install -d "${DESTDIR}${SHAREDIR}" "${DESTDIR}${BINDIR}"
install -m 644 data/agent-complete.mp3 "${DESTDIR}${SHAREDIR}/agent-complete.mp3"

binary_tmp="$(mktemp "${DESTDIR}${BINDIR}/.${BIN_NAME}.tmp.XXXXXX")"
trap 'rm -f -- "$binary_tmp"' EXIT
install -m 755 "${CARGO_TARGET_DIR:-target}/release/agent-notifier" "${binary_tmp}"
mv -f -- "${binary_tmp}" "${DESTDIR}${BINDIR}/${BIN_NAME}"
trap - EXIT

printf 'installed %s to %s and data to %s\n' "${BIN_NAME}" "${DESTDIR}${BINDIR}" "${DESTDIR}${SHAREDIR}"
