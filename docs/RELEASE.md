# Arcane Bridge — releases

## 1. Release

```bash
git pull
cd arcane_bridge
./scripts/release.sh
./scripts/release.sh minor
./scripts/release.sh major
```

Commits everything, bumps version, tags, pushes. CI builds Win/Mac/Linux and publishes GitHub Release assets.

**GitHub secrets** (repo Settings → Secrets → Actions):

Updater (minisign):

- `TAURI_SIGNING_PRIVATE_KEY` — full contents of `~/.tauri/arcane-bridge.key` (both lines)
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — password from `tauri signer generate`

macOS Developer ID + notarize (same values as Monitor):

- `CSC_LINK` — base64 of `.p12`
- `CSC_KEY_PASSWORD` — p12 password
- `APPLE_API_KEY` — base64 of AuthKey `.p8` file
- `APPLE_API_KEY_ID` — Key ID (e.g. `J46XST3TP9`)
- `APPLE_API_ISSUER` — Issuer UUID

The `pubkey` in `backend/tauri.conf.json` must match the minisign keypair.

## 2. Install (users)

Download the platform installer from the GitHub Release and install Bridge like any other app. Companion apps do **not** bundle Bridge.

## In-app updates

Bridge **auto-updates** on boot and every hour (download + install + restart). Tray → **Check for updates…** still works manually.

- Endpoint: `releases/latest/download/{{target}}-{{arch}}.json`
- CI uploads signed bundles + per-platform JSON on each `bridge-v*` release
- **Stable only** — `releases/latest` skips prereleases; beta tags do not auto-update in-app yet
- macOS needs Developer ID + notarized builds for install to succeed
