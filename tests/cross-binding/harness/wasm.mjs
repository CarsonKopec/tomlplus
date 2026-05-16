// WASM harness — imports tomlplus-wasm (nodejs build) and prints JSON.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

// We import from the workspace-local pkg-node build to avoid an npm
// install step in CI.
const wasmPath = resolve(
    process.argv[1],
    "..",
    "..",
    "..",
    "..",
    "crates",
    "tomlplus-wasm",
    "pkg-node",
    "tomlplus_wasm.js",
);
const { parse } = await import(pathToFileURL(wasmPath).href);

const src = readFileSync(process.argv[2], "utf8");
const doc = parse(src);
process.stdout.write(JSON.stringify(doc.config, null, 2) + "\n");
