// Node harness — parses a .tomlp file via the native module, prints JSON.
const fs   = require("node:fs");
const path = require("node:path");

// Import from the workspace-local napi-rs build so the orchestrator works
// without an `npm install` of the named package.
const tomlplus = require(path.resolve(__dirname, "..", "..", "..", "crates", "tomlplus-node", "index.js"));

const src = fs.readFileSync(process.argv[2], "utf8");
const doc = tomlplus.parse(src);
process.stdout.write(JSON.stringify(doc.config, null, 2) + "\n");
