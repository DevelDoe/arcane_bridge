#!/usr/bin/env bash
# Download Bridge installers from a GitHub Release into Caster / Guilds / Monitor.
#
# Usage:
#   ./scripts/stage-from-github-release.sh
#   ./scripts/stage-from-github-release.sh bridge-v0.1.2
#
# Optional env:
#   GITHUB_REPO=owner/arcane_bridge   (default: from git remote origin)
#   GITHUB_TOKEN=...                  (private repos / higher API rate limit)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
GIT_ROOT="$(git -C "${BRIDGE_ROOT}" rev-parse --show-toplevel)"
TAG="${1:-}"

gh_curl() {
  local url="$1"
  if [[ -n "${GITHUB_TOKEN:-}" ]]; then
    curl -fsSL -H "Authorization: Bearer ${GITHUB_TOKEN}" "${url}"
  else
    curl -fsSL "${url}"
  fi
}

gh_download() {
  local url="$1"
  local out="$2"
  if [[ -n "${GITHUB_TOKEN:-}" ]]; then
    curl -fsSL -H "Authorization: Bearer ${GITHUB_TOKEN}" -o "${out}" "${url}"
  else
    curl -fsSL -o "${out}" "${url}"
  fi
}

detect_github_repo() {
  if [[ -n "${GITHUB_REPO:-}" ]]; then
    echo "${GITHUB_REPO}"
    return
  fi
  local url
  url="$(git -C "${GIT_ROOT}" remote get-url origin 2>/dev/null || true)"
  if [[ -z "${url}" ]]; then
    return 1
  fi
  node -e "
    const raw = process.argv[1];
    const s = raw.trim().replace(/\\.git$/, '');
    const m = s.match(/github\\.com[:/](.+\\/[^/]+)\$/);
    if (!m) process.exit(1);
    console.log(m[1]);
  " "${url}"
}

parse_release_assets() {
  local tag="$1"
  local repo="$2"
  node -e "
    const tag = process.argv[1];
    const repo = process.argv[2];
    const data = JSON.parse(require('fs').readFileSync(0, 'utf8'));
    if (data.message === 'Not Found') {
      console.error('Release ' + tag + ' not found on GitHub yet.');
      console.error('Wait for CI to finish on: https://github.com/' + repo + '/actions');
      process.exit(1);
    }
    if (data.message) {
      console.error(data.message);
      process.exit(1);
    }
    const assets = (data.assets || []).filter(a => /bridge/i.test(a.name));
    if (!assets.length) {
      console.error('Release ' + tag + ' exists but has no Bridge installer assets yet.');
      console.error('CI may still be running — check Actions tab.');
      process.exit(1);
    }
    for (const a of assets) {
      console.log(a.name + '\t' + a.browser_download_url);
    }
  " "${tag}" "${repo}"
}

download_with_curl() {
  local repo="$1"
  local tag="$2"
  local dest="$3"
  local api="https://api.github.com/repos/${repo}/releases/tags/${tag}"

  local manifest
  manifest="$(
    gh_curl "${api}" | parse_release_assets "${tag}" "${repo}"
  )" || {
    echo "Failed to fetch release ${tag} from GitHub API." >&2
    exit 1
  }

  while IFS=$'\t' read -r name url; do
    [[ -z "${name}" ]] && continue
    echo "  downloading ${name}..."
    gh_download "${url}" "${dest}/${name}"
  done <<< "${manifest}"
}

resolve_latest_bridge_tag() {
  local repo="$1"
  gh_curl "https://api.github.com/repos/${repo}/releases?per_page=30" \
    | node -e "
      const list = JSON.parse(require('fs').readFileSync(0, 'utf8'));
      const hit = list.find(r => r.tag_name && r.tag_name.startsWith('bridge-v'));
      if (!hit) {
        console.error('No bridge-v* release found on GitHub yet.');
        process.exit(1);
      }
      console.log(hit.tag_name);
    "
}

REPO="$(detect_github_repo || true)"
if [[ -z "${REPO}" ]]; then
  echo "Set GITHUB_REPO=owner/repo or run from a git repo with github.com origin." >&2
  exit 1
fi

if [[ -z "${TAG}" ]]; then
  if command -v gh >/dev/null 2>&1; then
    TAG="$(gh release list --repo "${REPO}" --limit 20 --json tagName \
      | node -e "
        const tags = JSON.parse(require('fs').readFileSync(0,'utf8')).map(x => x.tagName);
        const hit = tags.find(t => t.startsWith('bridge-v'));
        if (!hit) { console.error('No bridge-v* release found'); process.exit(1); }
        console.log(hit);
      ")"
  else
    TAG="$(resolve_latest_bridge_tag "${REPO}")"
  fi
  echo "Using release: ${TAG}"
fi

WORKDIR="$(mktemp -d)"
trap 'rm -rf "${WORKDIR}"' EXIT

echo "Downloading ${TAG} from ${REPO}..."

if command -v gh >/dev/null 2>&1; then
  gh release download "${TAG}" --repo "${REPO}" --dir "${WORKDIR}" \
    --pattern 'Arcane-Bridge-*' 2>/dev/null \
    || gh release download "${TAG}" --repo "${REPO}" --dir "${WORKDIR}"
else
  download_with_curl "${REPO}" "${TAG}" "${WORKDIR}"
fi

shopt -s nullglob
ASSETS=("${WORKDIR}"/*)
if [[ ${#ASSETS[@]} -eq 0 ]]; then
  echo "No release assets downloaded." >&2
  echo "Check CI: https://github.com/${REPO}/actions" >&2
  exit 1
fi

if [[ -d "${GIT_ROOT}/arcane_caster" ]]; then
  MONOREPO_ROOT="${GIT_ROOT}"
elif [[ -d "${GIT_ROOT}/../arcane_caster" ]]; then
  MONOREPO_ROOT="$(cd "${GIT_ROOT}/.." && pwd)"
else
  MONOREPO_ROOT="${GIT_ROOT}"
fi

copy_to_apps() {
  local dest_root="$1"
  for APP_DIR in arcane_caster/backend arcane_guilds/backend arcane_monitor; do
    local DEST="${dest_root}/${APP_DIR}/resources/bridge"
    if [[ ! -d "${dest_root}/${APP_DIR}" ]]; then
      continue
    fi
    mkdir -p "${DEST}"
    rm -f "${DEST}"/*
    cp "${WORKDIR}"/* "${DEST}/"
    echo "  → ${DEST}"
  done
}

echo "Staging into app resource folders:"
if [[ -d "${MONOREPO_ROOT}/arcane_caster" ]]; then
  copy_to_apps "${MONOREPO_ROOT}"
else
  echo "  (arcane_caster not found — assets in ${WORKDIR})"
  ls -la "${WORKDIR}"
  exit 0
fi

echo ""
echo "Done. Build Caster / Guilds / Monitor to ship piggyback Bridge install."
