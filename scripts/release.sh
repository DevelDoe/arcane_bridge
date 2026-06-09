#!/usr/bin/env bash
# commit bridge changes → bump version → tag → push → CI
# Usage: ./scripts/release.sh [patch|minor|major] [--dry-run] [--no-push]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
GIT_ROOT="$(git -C "${BRIDGE_ROOT}" rev-parse --show-toplevel 2>/dev/null || true)"

BUMP="patch"
DRY_RUN=false
NO_PUSH=false

for arg in "$@"; do
  case "${arg}" in
    patch|minor|major) BUMP="${arg}" ;;
    --dry-run) DRY_RUN=true ;;
    --no-push) NO_PUSH=true ;;
    -h|--help)
      echo "Usage: ./scripts/release.sh [patch|minor|major] [--dry-run] [--no-push]"
      exit 0
      ;;
    *)
      echo "Unknown argument: ${arg}" >&2
      echo "Usage: ./scripts/release.sh [patch|minor|major] [--dry-run] [--no-push]" >&2
      exit 1
      ;;
  esac
done

if [[ -z "${GIT_ROOT}" ]] || ! git -C "${GIT_ROOT}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "Not inside a git repository." >&2
  exit 1
fi

CURRENT="$(node -p "require('${BRIDGE_ROOT}/backend/tauri.conf.json').version")"
VERSION="$(node -e "
  const v = '${CURRENT}'.split('-')[0].split('.').map(Number);
  const bump = '${BUMP}';
  if (bump === 'major') { v[0] = (v[0] || 0) + 1; v[1] = 0; v[2] = 0; }
  else if (bump === 'minor') { v[1] = (v[1] || 0) + 1; v[2] = 0; }
  else { v[2] = (v[2] || 0) + 1; }
  console.log(v.join('.'));
")"
TAG="bridge-v${VERSION}"
BRANCH="$(git -C "${GIT_ROOT}" branch --show-current)"

if [[ -f "${GIT_ROOT}/arcane_bridge/backend/tauri.conf.json" ]]; then
  BRIDGE_PREFIX="arcane_bridge"
elif [[ -f "${GIT_ROOT}/backend/tauri.conf.json" ]]; then
  BRIDGE_PREFIX="."
else
  echo "Cannot find tauri.conf.json under git root" >&2
  exit 1
fi

BRIDGE_PATH="${BRIDGE_PREFIX}"

run() {
  if [[ "${DRY_RUN}" == true ]]; then
    echo "[dry-run] $*"
  else
    "$@"
  fi
}

echo "==> Arcane Bridge ${CURRENT} → ${VERSION} (${TAG}, ${BUMP})"

run node "${SCRIPT_DIR}/sync-version.mjs" "${VERSION}"
run git -C "${GIT_ROOT}" add -A -- "${BRIDGE_PATH}"

if git -C "${GIT_ROOT}" diff --cached --quiet -- "${BRIDGE_PATH}"; then
  echo "Nothing to commit under ${BRIDGE_PATH}." >&2
  exit 1
fi

run git -C "${GIT_ROOT}" commit -m "bridge: release ${VERSION}" -- "${BRIDGE_PATH}"

if git -C "${GIT_ROOT}" rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
  echo "Tag ${TAG} already exists." >&2
  exit 1
fi

run git -C "${GIT_ROOT}" tag "${TAG}"

if [[ "${NO_PUSH}" == true ]]; then
  echo "Done (not pushed)."
  exit 0
fi

run git -C "${GIT_ROOT}" push origin "${BRANCH}"
run git -C "${GIT_ROOT}" push origin "${TAG}"

echo ""
echo "✓ ${TAG} pushed. Wait for CI, then:"
echo "  ./scripts/stage-from-github-release.sh"
