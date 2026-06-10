# Arcane Bridge — releases

## 1. Release

```bash
git pull
cd arcane_bridge
./scripts/release.sh        
./scripts/release.sh minor
./scripts/release.sh major
```

Commits everything, bumps version, tags, pushes. CI builds Win/Mac/Linux.

**GitHub secrets** (repo Settings → Secrets → Actions):

- `TAURI_SIGNING_PRIVATE_KEY` — full contents of `~/.tauri/arcane-bridge.key` (both lines)
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — password from `tauri signer generate`

The `pubkey` in `backend/tauri.conf.json` must match that keypair.

## 2. Stage (CI green)

```bash
./scripts/stage-from-github-release.sh
```

Installers → Monitor / Caster / Guilds `resources/bridge/`.

## 3. Ship apps

Commit `resources/bridge/` in each app. Build on your hosts.

## In-app updates

Bridge checks GitHub Releases on demand (tray → **Check for updates…**).

- Endpoint: `releases/latest/download/{{target}}-{{arch}}.json`
- CI uploads signed bundles + per-platform JSON on each `bridge-v*` release
- **Stable only** — `releases/latest` skips prereleases; beta tags do not auto-update in-app yet
