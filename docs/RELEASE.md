# Arcane Bridge — releases

## What ships

| Artifact | Platform |
|----------|----------|
| `Arcane Bridge_*_x64-setup.exe` | Windows (NSIS) |
| `Arcane Bridge_*.dmg` | macOS |

The tray app bundles `hub/arcane-bridge.mjs` inside the installer. **Node 18+** must still be on the user’s `PATH` (tray spawns `node` on the bundled script).

## Tag and publish

```bash
git tag bridge-v0.1.0
git push origin bridge-v0.1.0
```

GitHub Actions (`.github/workflows/release.yml`) builds Windows + macOS and uploads artifacts.

CI uploads **GitHub Actions artifacts** per platform. Add R2 upload to the workflow when ready (same secrets pattern as Caster/Guilds).

## Local build

```bash
cd hub && npm ci && npm run build
cd ../backend && cargo tauri build --bundles nsis    # Windows
cd ../backend && cargo tauri build --bundles app,dmg # macOS
```

## Piggyback install (Caster / Guilds / Monitor)

See [PIGGYBACK.md](PIGGYBACK.md). Stage Bridge installers into each app before building them:

```bash
bash arcane_bridge/scripts/stage-installers-for-apps.sh
```
