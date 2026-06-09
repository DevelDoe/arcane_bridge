#!/usr/bin/env bash
# Copy Bridge installers into Caster / Guilds / Monitor resource folders before app release builds.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BRIDGE_BACKEND="${ROOT}/arcane_bridge/backend"
STAGE="${ROOT}/arcane_bridge/dist/installers"
VERSION="$(node -p "require('${ROOT}/arcane_bridge/backend/tauri.conf.json').version")"

mkdir -p "${STAGE}"
rm -f "${STAGE}"/*

OS="$(uname -s)"
case "${OS}" in
  Darwin)
    APP="${BRIDGE_BACKEND}/target/release/bundle/macos/Arcane Bridge.app"
    OUT="${STAGE}/Arcane-Bridge-${VERSION}-macos.app.tar.gz"
    if [[ ! -d "${APP}" ]]; then
      echo "Missing ${APP} — run: cd arcane_bridge/backend && cargo tauri build --bundles app"
      exit 1
    fi
    tar czf "${OUT}" -C "$(dirname "${APP}")" "Arcane Bridge.app"
    echo "Staged ${OUT}"
    ;;
  MINGW*|MSYS*|CYGWIN*|Windows*)
    EXE="$(ls -1 "${BRIDGE_BACKEND}"/target/release/bundle/nsis/*setup*.exe 2>/dev/null | head -n 1 || true)"
    if [[ -z "${EXE}" ]]; then
      echo "Missing NSIS installer — run: cd arcane_bridge/backend && cargo tauri build --bundles nsis"
      exit 1
    fi
    cp "${EXE}" "${STAGE}/Arcane-Bridge-${VERSION}-windows-setup.exe"
    echo "Staged ${STAGE}/Arcane-Bridge-${VERSION}-windows-setup.exe"
    ;;
  Linux)
    DEB="$(ls -1 "${BRIDGE_BACKEND}"/target/release/bundle/deb/*.deb 2>/dev/null | head -n 1 || true)"
    if [[ -z "${DEB}" ]]; then
      echo "Missing deb — run: cd arcane_bridge/backend && cargo tauri build --bundles deb"
      exit 1
    fi
    cp "${DEB}" "${STAGE}/Arcane-Bridge-${VERSION}-linux-amd64.deb"
    echo "Staged ${STAGE}/Arcane-Bridge-${VERSION}-linux-amd64.deb"
    ;;
  *)
    echo "Unsupported OS for staging: ${OS}"
    exit 1
    ;;
esac

for APP_DIR in arcane_caster/backend arcane_guilds/backend arcane_monitor; do
  DEST="${ROOT}/${APP_DIR}/resources/bridge"
  mkdir -p "${DEST}"
  rm -f "${DEST}"/*
  cp -R "${STAGE}/." "${DEST}/"
  echo "Copied installers → ${DEST}"
done
