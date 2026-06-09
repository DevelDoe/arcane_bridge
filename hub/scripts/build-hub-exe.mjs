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

if (process.platform === "win32") {
    hideWindowsConsoleExe(built);
} else {
    fs.chmodSync(built, 0o755);
}

const mb = Math.round(fs.statSync(built).size / 1024 / 1024);
console.log(`[hub-exe] ready: ${built} (${mb} MB)`);

function hideWindowsConsoleExe(exePath) {
    const editbin = findEditbin();
    if (!editbin) {
        throw new Error(
            "[hub-exe] editbin not found — install Visual Studio Build Tools so arcane-bridge-hub.exe is built without a console window",
        );
    }

    console.log(`[hub-exe] hiding console window for ${path.basename(exePath)}`);
    const patch = spawnSync(
        editbin,
        ["/SUBSYSTEM:WINDOWS", "/ENTRY:mainCRTStartup", exePath],
        { stdio: "inherit" },
    );
    if (patch.error) {
        throw patch.error;
    }
    if (patch.status !== 0) {
        throw new Error(`editbin exited with code ${patch.status ?? "unknown"}`);
    }
}

function findEditbin() {
    const onPath = spawnSync("where", ["editbin"], { encoding: "utf8", shell: true });
    if (onPath.status === 0) {
        const hit = onPath.stdout.split(/\r?\n/).map((line) => line.trim()).find(Boolean);
        if (hit && fs.existsSync(hit)) {
            return hit;
        }
    }

    const programFiles = process.env["ProgramFiles(x86)"] || process.env.ProgramFiles;
    if (!programFiles) {
        return null;
    }

    const vswhere = path.join(programFiles, "Microsoft Visual Studio", "Installer", "vswhere.exe");
    if (!fs.existsSync(vswhere)) {
        return null;
    }

    const locate = spawnSync(
        vswhere,
        ["-latest", "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-find", "**/editbin.exe"],
        { encoding: "utf8" },
    );
    if (locate.status !== 0) {
        return null;
    }

    const found = locate.stdout.split(/\r?\n/).map((line) => line.trim()).find(Boolean);
    return found && fs.existsSync(found) ? found : null;
}
