#!/usr/bin/env node
import * as esbuild from "esbuild";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");
const outDir = path.join(root, "dist");
const outfile = path.join(outDir, "arcane-bridge.mjs");

fs.mkdirSync(outDir, { recursive: true });

await esbuild.build({
    entryPoints: [path.join(root, "src", "index.js")],
    bundle: true,
    platform: "node",
    target: "node18",
    format: "esm",
    outfile,
    banner: {
        js: "#!/usr/bin/env node",
    },
});

try {
    fs.chmodSync(outfile, 0o755);
} catch {
    /* windows */
}

console.log(`[arcane-bridge] built ${outfile}`);
