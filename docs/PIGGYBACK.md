# Piggyback Bridge install

Caster, Guilds, and Monitor can install **Arcane Bridge** on first launch if the hub is down.

## Flow

1. App starts → probe `127.0.0.1:47991`
2. Hub up → continue
3. Bridge installed but hub down → launch Bridge tray
4. Not installed → run bundled installer from `resources/bridge/`
5. Existing TCP client retry loop connects when hub is ready

## Stage installers before app release build

Get Bridge installers from **either**:

- **GitHub Releases** — download `*-windows-setup.exe` and `*-macos.app.tar.gz` from the latest tag, **or**
- **Local build** — see [RELEASE.md](RELEASE.md)

Then copy into all apps:

```bash
# Option A: local build + stage script
cd arcane_bridge/hub && npm ci && npm run build
cd ../backend && cargo tauri build --bundles app,dmg   # macOS
# or --bundles nsis (Windows) / deb (Linux)
bash arcane_bridge/scripts/stage-installers-for-apps.sh

# Option B: place GitHub Release assets manually into each app's resources/bridge/

# Then build Caster / Guilds / Monitor as usual
```

| OS | Bundled file | Install target |
|----|--------------|----------------|
| **macOS** | `Arcane-Bridge-*-macos.app.tar.gz` | `/Applications` |
| **Windows** | `Arcane-Bridge-*-windows-setup.exe` | silent `/S` |
| **Linux** | `Arcane-Bridge-*-linux-amd64.deb` | `~/.local/share/arcane-bridge` (no sudo) |

Without staged installers, apps log a message and keep retrying (same as before).
