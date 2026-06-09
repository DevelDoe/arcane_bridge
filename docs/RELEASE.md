# Arcane Bridge — releases

## 1. Release

```bash
git pull
cd arcane_bridge
./scripts/release.sh patch
```

Commits all bridge changes, bumps version, tags `bridge-vX.Y.Z`, pushes. CI builds Win/Mac/Linux.

Version: `patch` · `minor` · `0.2.1` · `bridge-v0.2.1`

Flags: `--dry-run` · `--no-push`

## 2. Stage into apps (after CI green)

```bash
./scripts/stage-from-github-release.sh bridge-vX.Y.Z
```

Copies installers → Monitor / Caster / Guilds `resources/bridge/`.

Omit tag to use latest `bridge-v*` on GitHub.

## 3. Ship apps

Commit `resources/bridge/` in each app. Build and release each app on your hosts.

---

**Local test only (no release):** `./scripts/build-local.sh`

**What ships:** one tray app per platform. Hub in-process on `127.0.0.1:47991` — no sidecar exe, no Node on user machines.

**CI assets:** `*-windows-setup.exe` · `*-macos.dmg` · `*-macos.app.tar.gz` · `*-linux-amd64.deb`
