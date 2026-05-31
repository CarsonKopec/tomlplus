# TOML+

> TOML, plus block dictionaries, annotations, and variables.

[![CI](https://github.com/CarsonKopec/tomlplus/actions/workflows/ci.yml/badge.svg)](https://github.com/CarsonKopec/tomlplus/actions/workflows/ci.yml)
[![Release](https://github.com/CarsonKopec/tomlplus/actions/workflows/release.yml/badge.svg)](https://github.com/CarsonKopec/tomlplus/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/github/license/CarsonKopec/tomlplus)](LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/CarsonKopec/tomlplus?include_prereleases&label=latest&color=blue)](https://github.com/CarsonKopec/tomlplus/releases)
[![Bindings](https://img.shields.io/badge/bindings-8-blueviolet)](#install)

```toml
[vars]
base_url = "https://api.example.com"

[server]
@type: int
@min: 1
@max: 65535
port = $ENV.PORT ?? 8080

cors = #{
    @required
    access-control-allow-origin = $base_url
}#
```

A pragmatic config language: every `.tomlp` file is also a valid mental model
for a developer who knows TOML. Three additions:

| Feature | Why |
| --- | --- |
| **Block dictionaries** (`#{ … }#`) | Inline-nested config without a new `[section]` header |
| **Annotations** (`@required`, `@type: int`, `@enum: […]`, `@deprecated("…")`) | Schema declared next to the value, validated at parse time |
| **Variables** (`$base`, `$ENV.X ?? fallback`, `$base + "/v1"`) | DRY repeated values, pull env, do tiny arithmetic |

## Install

One Rust core, **eight** first-class bindings — every one produces
byte-identical JSON for any `.tomlp` file (enforced by a cross-binding
integration test on every push).

| Language | Install | Version |
| --- | --- | --- |
| **Python** ≥3.8 | `pip install tomlplus` | [![PyPI](https://img.shields.io/pypi/v/tomlplus?label=&color=blue)](https://pypi.org/project/tomlplus/) |
| **Node** / Deno / Bun | `npm install tomlplus` (native)<br>`npm install tomlplus-wasm` (universal) | [![npm](https://img.shields.io/npm/v/tomlplus?label=tomlplus&color=blue)](https://www.npmjs.com/package/tomlplus) [![npm](https://img.shields.io/npm/v/tomlplus-wasm?label=wasm&color=blue)](https://www.npmjs.com/package/tomlplus-wasm) |
| **Rust** | `cargo add tomlplus-syntax` | [![crates.io](https://img.shields.io/crates/v/tomlplus-syntax?label=&color=blue)](https://crates.io/crates/tomlplus-syntax) |
| **Java** ≥17 | `io.github.carsonkopec:tomlplus-java` | [![Maven Central](https://img.shields.io/maven-central/v/io.github.carsonkopec/tomlplus-java?label=&color=blue)](https://central.sonatype.com/artifact/io.github.carsonkopec/tomlplus-java) |
| **.NET** 8 | `dotnet add package Tomlplus` | [![NuGet](https://img.shields.io/nuget/v/Tomlplus?label=&color=blue)](https://www.nuget.org/packages/Tomlplus) |
| **Go** | `go get github.com/CarsonKopec/tomlplus/bindings/tomlplus-go` | via git tag |
| **Ruby** | `gem install tomlplus` | [![Gem](https://img.shields.io/gem/v/tomlplus?label=&color=blue)](https://rubygems.org/gems/tomlplus) |
| **C / C++ / others** | Download `tomlplus-ffi-<ver>-<arch>.zip` from [Releases](https://github.com/CarsonKopec/tomlplus/releases) | per-OS tarball |

Tools:

```bash
cargo install tomlpr               # parse / validate / fmt / vars CLI
cargo install tomlplus-lsp         # language server for editors
```

## Editor support

* **VSCodium / Cursor / Windsurf / Theia / Gitpod** — install **TOML+** from [Open VSX](https://open-vsx.org/extension/CarsonKopec/tomlplus). Ships syntax highlighting, semantic tokens, hover, completion, diagnostics, inlay hints, goto-def, formatting, and a colour picker for `#RRGGBB` strings.
* **Microsoft VS Code** — manual install for now:
  ```pwsh
  # Download the .vsix from the latest GitHub Release, then:
  code --install-extension tomlplus-2.0.0.vsix
  ```
  (Marketplace listing is on the roadmap pending Azure DevOps setup.)
* **Neovim / Helix / Zed / Sublime / IntelliJ / Emacs** — point any LSP client at the `tomlplus-lsp` binary, language id `tomlplus`, file extension `.tomlp`. Neovim example:

  ```lua
  vim.lsp.start({
      name = "tomlplus",
      cmd  = { "tomlplus-lsp" },
      root_dir = vim.fs.dirname(vim.fs.find({ ".git" }, { upward = true })[1]),
  })
  ```

## Repository layout

```
.
├── Cargo.toml                     # Cargo workspace
├── release.py                     # build/test/package/publish dispatcher (Python + rich)
├── PUBLISHING.md                  # per-target release commands
├── CHANGELOG.md
├── .github/workflows/             # CI matrix + tag-driven release pipeline
├── crates/                        # Cargo crates (Rust-native bindings)
│   ├── tomlplus-syntax/           # lexer + parser + validator + dumper (the language)
│   ├── tomlplus-lsp/              # tower-lsp language server
│   ├── tomlplus-cli/              # `tomlpr` command-line tool
│   ├── tomlplus-ffi/              # C ABI cdylib + tomlplus.h header
│   ├── tomlplus-python/           # PyO3 wheel (drop-in `pip install tomlplus`)
│   ├── tomlplus-node/             # napi-rs Node module
│   └── tomlplus-wasm/             # wasm-pack module (browser / Deno / Bun / Workers)
├── bindings/                      # Non-Rust bindings (consume tomlplus-ffi)
│   ├── tomlplus-go/               # cgo Go module
│   ├── tomlplus-ruby/             # FFI Ruby gem
│   ├── tomlplus-java/             # Gradle artefact, published io.github.carsonkopec:tomlplus-java
│   └── tomlplus-dotnet/           # P/Invoke .NET class library
├── editors/vscode/                # VS Code client extension (esbuild-bundled)
├── tests/cross-binding/           # one fixture parsed by every binding, JSON-compared
└── scripts/                       # one-off maintenance scripts (availability check, …)
```

`tomlplus-syntax` owns the language — no I/O, no async, no LSP types. Every
other crate consumes it. Adding a new language is mechanical: a new
`crates/tomlplus-<lang>/` if the ABI talks to Rust directly (Python, Node,
WASM), or a new `bindings/tomlplus-<lang>/` if it goes through the C FFI
(Go, Ruby, Java, .NET, Swift, …).

## Develop

```pwsh
# One-time:
winget install Rustlang.Rustup     # or https://rustup.rs/

# Then the dispatcher orchestrates everything:
py -3 release.py build              # build everything
py -3 release.py test               # run every test suite (incl. cross-binding)
py -3 release.py test -t python     # one target
py -3 release.py package -t all     # produce shippable artefacts under release/
py -3 release.py publish -t all --dry-run

# Quick checks:
cargo test --workspace              # Rust unit tests (fast)
py -3 scripts/check-package-availability.py   # are our names still free?
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the dev workflow and
[`PUBLISHING.md`](PUBLISHING.md) for per-registry publish commands.

## Status

Pre-2.0.0 release. The code is feature-complete and CI is green across
Ubuntu / Windows / macOS for every binding; first registry pushes (PyPI,
npm, crates.io, NuGet, RubyGems, Maven Central, Open VSX) are pending
manual one-time account setup. See [CHANGELOG.md](CHANGELOG.md) for what's
in 2.0.0.

## License

[MIT](LICENSE) © Carson Kopec.
