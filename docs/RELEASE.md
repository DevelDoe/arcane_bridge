# Arcane Bridge — releases

## What ships

| Artifact | Platform | Use |
|----------|----------|-----|
| `Arcane-Bridge-*-windows-setup.exe` | Windows (NSIS) | Direct install |
| `Arcane-Bridge-*-macos.dmg` | macOS | Direct install |
| `Arcane-Bridge-*-macos.app.tar.gz` | macOS | Piggyback bundle |
| `Arcane-Bridge-*-linux-amd64.deb` | Linux | Direct install + piggyback |

The tray app bundles `hub/arcane-bridge.mjs` inside the installer. **Node 18+** must still be on the user’s `PATH`.

Linux piggyback (Caster / Guilds / Monitor) extracts the `.deb` to `~/.local/share/arcane-bridge` — no `sudo` required.

## Distribution options (pick what fits)

| Channel | Best for |
|---------|----------|
| **[GitHub Releases](https://github.com/OWNER/arcane_bridge/releases)** | Users downloading Bridge directly |
| **CI workflow artifacts** | Internal testing before tagging |
| **Piggyback in Caster / Guilds / Monitor** | Auto-install Bridge on first launch |
| **R2 / updater CDN** *(future)* | Silent auto-updates |

GitHub Releases is an **alternative** — not a replacement for piggyback or a future updater feed.

## Tag and publish (GitHub Release)

```bash
git tag bridge-v0.1.0
git push origin bridge-v0.1.0
```

CI builds **Windows + macOS + Linux**, uploads workflow artifacts, and creates a GitHub Release on tag push.

Manual `workflow_dispatch` → artifacts only, no GitHub Release.

## Local build

```bash
cd hub && npm ci && npm run build
cd ../backend && cargo tauri build --bundles nsis       # Windows
cd ../backend && cargo tauri build --bundles app,dmg    # macOS
cd ../backend && cargo tauri build --bundles deb        # Linux
```

## Piggyback install (Caster / Guilds / Monitor)

See [PIGGYBACK.md](PIGGYBACK.md). Stage installers before building other apps:

```bash
bash arcane_bridge/scripts/stage-installers-for-apps.sh
```

Or download assets from a GitHub Release into each app’s `resources/bridge/`.
