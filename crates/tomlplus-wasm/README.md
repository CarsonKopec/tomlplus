# tomlplus-wasm

WebAssembly bindings for the TOML+ language core. Runs **anywhere** WASM
runs: browsers, Node, Deno, Bun, Cloudflare Workers, Fastly Compute@Edge,
etc. Single artefact, no native binaries to ship.

## Install

```bash
npm install tomlplus-wasm
```

## Build (from source)

You need `wasm-pack` once: `cargo install wasm-pack`. Then:

```pwsh
cd crates\tomlplus-wasm

# Browser ES modules (typical web/Vite/webpack target)
wasm-pack build --release --target web      --out-dir pkg-web

# Bundler-friendly (webpack, Rollup, Vite, esbuild)
wasm-pack build --release --target bundler  --out-dir pkg-bundler

# Node.js (CommonJS)
wasm-pack build --release --target nodejs   --out-dir pkg-node

# Deno / Bun / browser plain script
wasm-pack build --release --target no-modules --out-dir pkg-no-modules
```

Each output directory is a complete npm package with `package.json`,
`.wasm`, `.js`, and `.d.ts`.

## Usage

```javascript
import init, { parse, validate, dumps } from "tomlplus-wasm";

await init();   // browser: load the .wasm file

const doc = parse(`[server]
@type: int
port = 8080`);

console.log(doc.resolve("server.port"));   // 8080
console.log(doc.hasAnnotation("server.port", "type")); // true
validate(doc);                               // throws if invalid
console.log(dumps(doc));
```

In Node / Deno, `await init()` is unnecessary — just import and call.

API mirrors `tomlplus` (the napi-rs Node module): `parse`, `validate`,
`validateAll`, `dumps`, `version`; `TomlplusDocument` has `.resolve()`,
`.annotations()`, `.hasAnnotation()`, `.tags()`, `.requiredKeys()`,
`.deprecatedKeys()`, `.keysWithTag()`, plus `.config`/`.vars`/`.meta` getters.
