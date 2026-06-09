#!/usr/bin/env bash
# Build Bridge on THIS machine only (dev / smoke test). Does not tag or push.
#
# Usage:
#   ./scripts/build-local.sh
#   ./scripts/build-local.sh 0.1.3    # sync version first
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

if [[ -n "${1:-}" ]]; then
  node "${SCRIPT_DIR}/sync-version.mjs" "$1"
fi

OS="$(uname -s)"
echo "==> Building Arcane Bridge (${OS})..."
cd "${BRIDGE_ROOT}/backend"

case "${OS}" in
  Darwin)
    cargo tauri build --bundles app,dmg
    ;;
  Linux)
    cargo tauri build --bundles deb
    ;;
  MINGW*|MSYS*|CYGWIN*|Windows*)
    cargo tauri build --bundles nsis
    ;;
  *)
    echo "Unsupported OS: ${OS}" >&2
    exit 1
    ;;
esac

echo ""
echo "✓ Local build finished. Output under backend/target/release/bundle/"
