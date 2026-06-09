# Piggyback Bridge install

Caster, Guilds, and Monitor can install **Arcane Bridge** on first launch if the hub is down.

## Flow

1. App starts → probe `127.0.0.1:47991`
2. Hub up → continue
3. Bridge installed but hub down → launch Bridge tray
4. Not installed → run bundled installer from `resources/bridge/`
5. Existing TCP client retry loop connects when hub is ready

## Stage installers before app release build

```bash
# 1. Build Bridge
cd arcane_bridge/hub && npm ci && npm run build
cd ../backend && cargo tauri build --bundles app,dmg   # macOS
# or --bundles nsis on Windows

# 2. Copy into all apps
bash arcane_bridge/scripts/stage-installers-for-apps.sh

# 3. Build Caster / Guilds / Monitor as usual
```

**macOS bundle:** `Arcane-Bridge.app.tar.gz` (extracts to `/Applications`)

**Windows bundle:** `arcane-bridge-setup.exe` (silent `/S`)

Without staged installers, apps log a message and keep retrying (same as before).
