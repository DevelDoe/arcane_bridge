#!/usr/bin/env node
/**
 * Sync version into tauri.conf.json before a release build.
 *
 * Usage:
 *   node scripts/sync-version.mjs 0.1.2
 *   node scripts/sync-version.mjs bridge-v0.1.2
 */
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");

let raw = process.argv[2];
if (!raw) {
    console.error("Usage: node scripts/sync-version.mjs <version|bridge-vX.Y.Z>");
    process.exit(1);
}

const version = raw.startsWith("bridge-v") ? raw.slice("bridge-v".length) : raw;
if (!/^\d+\.\d+\.\d+/.test(version)) {
    console.error(`Invalid version: ${version}`);
    process.exit(1);
}

const tauriPath = path.join(root, "backend", "tauri.conf.json");
const tauri = JSON.parse(fs.readFileSync(tauriPath, "utf8"));
tauri.version = version;
fs.writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);

console.log(`[sync-version] set version ${version} in tauri.conf.json`);
