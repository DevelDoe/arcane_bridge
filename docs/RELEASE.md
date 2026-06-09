# Arcane Bridge — releases

## Quick start (automated)

**Ship a new version** (one command — CI builds Win/Mac/Linux + GitHub Release):

```bash
cd arcane_bridge
./scripts/release.sh patch          
# or: bash scripts/release.sh patch
# or: ./scripts/release.sh 0.2.0
# or: ./scripts/release.sh patch --dry-run
```

**After CI finishes** — pull Bridge installers into Caster / Guilds / Monitor:

```bash
./scripts/stage-from-github-release.sh bridge-v0.1.2
# or omit tag for latest bridge-v* release (uses curl + GitHub API — no gh required)
```

**Local test build** (this machine only, no git tag):

```bash
./scripts/build-local.sh
```

That’s it for the normal flow.

---

## What CI ships

| Artifact | Platform |
|----------|----------|
| `Arcane-Bridge-*-windows-setup.exe` | Windows |
| `Arcane-Bridge-*-macos.dmg` | macOS |
| `Arcane-Bridge-*-macos.app.tar.gz` | macOS piggyback |
| `Arcane-Bridge-*-linux-amd64.deb` | Linux |

Requires **Node 18+** on user `PATH`.

## Scripts

| Script | Purpose |
|--------|---------|
| `scripts/release.sh` | bump version, commit, tag `bridge-v*`, push → triggers CI |
| `scripts/stage-from-github-release.sh` | `gh release download` → app `resources/bridge/` |
| `scripts/build-local.sh` | hub + tray build on current OS only |
| `scripts/sync-version.mjs` | write version into `tauri.conf.json` + hub `package.json` |

## Requirements

- `git`, `node`, `curl`
- `gh` optional (staging works without it)
- `GITHUB_REPO=owner/repo` if git remote isn’t the Bridge repo
- Tag format: `bridge-v0.1.2`
- CI syncs version from tag before `cargo tauri build`

## Manual / legacy

Local per-OS builds and `stage-installers-for-apps.sh` (from local `target/`) still work — see git history or `PIGGYBACK.md`.
