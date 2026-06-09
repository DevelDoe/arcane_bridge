#!/usr/bin/env bash
# One-command Bridge release: bump version → commit → tag → push → CI builds all platforms.
#
# Usage:
#   ./scripts/release.sh 0.1.3
#   ./scripts/release.sh patch
#   ./scripts/release.sh minor
#   ./scripts/release.sh 0.1.3 --dry-run
#   ./scripts/release.sh 0.1.3 --no-push
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
GIT_ROOT="$(git -C "${BRIDGE_ROOT}" rev-parse --show-toplevel 2>/dev/null || true)"

DRY_RUN=false
NO_PUSH=false
VERSION_ARG=""

for arg in "$@"; do
  case "${arg}" in
    --dry-run) DRY_RUN=true ;;
    --no-push) NO_PUSH=true ;;
    -h|--help)
      sed -n '2,12p' "$0"
      exit 0
      ;;
    *)
      if [[ -z "${VERSION_ARG}" ]]; then
        VERSION_ARG="${arg}"
      else
        echo "Unknown argument: ${arg}" >&2
        exit 1
      fi
      ;;
  esac
done

if [[ -z "${VERSION_ARG}" ]]; then
  echo "Usage: ./scripts/release.sh <version|patch|minor> [--dry-run] [--no-push]" >&2
  exit 1
fi

if [[ -z "${GIT_ROOT}" ]] || ! git -C "${GIT_ROOT}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "Not inside a git repository (expected arcane_bridge or parent monorepo)." >&2
  exit 1
fi

resolve_version() {
  local arg="$1"
  local current
  current="$(node -p "require('${BRIDGE_ROOT}/backend/tauri.conf.json').version")"

  case "${arg}" in
    patch)
      node -e "
        const v = '${current}'.split('-')[0].split('.').map(Number);
        v[2] = (v[2] || 0) + 1;
        console.log(v.join('.'));
      "
      ;;
    minor)
      node -e "
        const v = '${current}'.split('-')[0].split('.').map(Number);
        v[1] = (v[1] || 0) + 1;
        v[2] = 0;
        console.log(v.join('.'));
      "
      ;;
    *)
      if [[ "${arg}" == bridge-v* ]]; then
        echo "${arg#bridge-v}"
      else
        echo "${arg}"
      fi
      ;;
  esac
}

VERSION="$(resolve_version "${VERSION_ARG}")"
TAG="bridge-v${VERSION}"
BRANCH="$(git -C "${GIT_ROOT}" branch --show-current)"

if [[ -f "${GIT_ROOT}/arcane_bridge/backend/tauri.conf.json" ]]; then
  FILES=(arcane_bridge/backend/tauri.conf.json arcane_bridge/hub/package.json)
elif [[ -f "${GIT_ROOT}/backend/tauri.conf.json" ]]; then
  FILES=(backend/tauri.conf.json hub/package.json)
else
  echo "Cannot find tauri.conf.json under git root" >&2
  exit 1
fi

run() {
  if [[ "${DRY_RUN}" == true ]]; then
    echo "[dry-run] $*"
  else
    "$@"
  fi
}

echo "==> Arcane Bridge release ${VERSION} (tag ${TAG})"
echo "    git root: ${GIT_ROOT}"
echo "    branch:   ${BRANCH}"

run node "${SCRIPT_DIR}/sync-version.mjs" "${VERSION}"

for f in "${FILES[@]}"; do
  if [[ -n "${f}" && -f "${GIT_ROOT}/${f}" ]]; then
    run git -C "${GIT_ROOT}" add "${f}"
  fi
done

if git -C "${GIT_ROOT}" diff --cached --quiet; then
  echo "No version file changes to commit (already ${VERSION}?)"
else
  run git -C "${GIT_ROOT}" commit -m "bridge: release ${VERSION}"
fi

if git -C "${GIT_ROOT}" rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
  echo "Tag ${TAG} already exists. Delete it first or pick a new version." >&2
  exit 1
fi

run git -C "${GIT_ROOT}" tag "${TAG}"

if [[ "${NO_PUSH}" == true ]]; then
  echo ""
  echo "Done (not pushed). Run:"
  echo "  git -C \"${GIT_ROOT}\" push origin ${BRANCH}"
  echo "  git -C \"${GIT_ROOT}\" push origin ${TAG}"
  exit 0
fi

run git -C "${GIT_ROOT}" push origin "${BRANCH}"
run git -C "${GIT_ROOT}" push origin "${TAG}"

echo ""
echo "✓ Pushed ${TAG}. GitHub Actions will build Windows + macOS + Linux and publish a Release."
echo ""
echo "When CI finishes, stage installers into other apps:"
echo "  ${SCRIPT_DIR}/stage-from-github-release.sh ${TAG}"
