#!/usr/bin/env node
/**
 * Package the bundled hub into a single native executable (no user Node install).
 */
import { spawnSync } from "child_process";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const hubRoot = path.join(__dirname, "..");
const distDir = path.join(hubRoot, "dist");
const bundleDir = path.join(distDir, "hub-bundle");
const entry = path.join(distDir, "arcane-bridge.cjs");

const targets = {
    win32: "node22-win-x64",
    darwin: process.arch === "arm64" ? "node22-macos-arm64" : "node22-macos-x64",
    linux: "node22-linux-x64",
};

const target = targets[process.platform];
if (!target) {
    throw new Error(`No pkg target for platform ${process.platform}`);
}

if (!fs.existsSync(entry)) {
    throw new Error(`Missing ${entry} — run npm run build first`);
}

fs.rmSync(bundleDir, { recursive: true, force: true });
fs.mkdirSync(bundleDir, { recursive: true });

const outputBase = path.join(bundleDir, "arcane-bridge-hub");
const pkgCli = path.join(hubRoot, "node_modules", "@yao-pkg", "pkg", "lib-es5", "bin.js");

console.log(`[hub-exe] packaging ${entry} for ${target}`);
const result = spawnSync(
    process.execPath,
    [pkgCli, entry, "--sea", "-t", target, "-o", outputBase],
    { stdio: "inherit", cwd: hubRoot },
);
if (result.error) {
    throw result.error;
}
if (result.status !== 0) {
    throw new Error(`pkg exited with code ${result.status ?? "unknown"}`);
}

const builtName = process.platform === "win32" ? "arcane-bridge-hub.exe" : "arcane-bridge-hub";
const built = path.join(bundleDir, builtName);
if (!fs.existsSync(built)) {
    throw new Error(`pkg did not produce ${built}`);
}

if (process.platform !== "win32") {
    fs.chmodSync(built, 0o755);
}

const mb = Math.round(fs.statSync(built).size / 1024 / 1024);
console.log(`[hub-exe] ready: ${built} (${mb} MB)`);
