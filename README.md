# TOML+

[![CI](https://github.com/CarsonKopec/tomlplus/actions/workflows/ci.yml/badge.svg)](https://github.com/CarsonKopec/tomlplus/actions/workflows/ci.yml)
[![Release](https://github.com/CarsonKopec/tomlplus/actions/workflows/release.yml/badge.svg)](https://github.com/CarsonKopec/tomlplus/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/github/license/CarsonKopec/tomlplus)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/CarsonKopec/tomlplus?include_prereleases&color=blue)](https://github.com/CarsonKopec/tomlplus/releases)

TOML, but with three things it didn't have:

```toml
[vars]                                  # variables you can reference later
base_url = "https://api.example.com"

[server]
@type: int                              # annotations, validated at parse time
@min: 1
@max: 65535
port = $ENV.PORT ?? 8080                # env vars with a fallback

cors = #{                               # block dictionaries
  @required
  access-control-allow-origin = $base_url
}#
```

That's the whole pitch. Every `.toml` file is a valid `.tomlp`, so adoption
is one rename. The three additions:

- **Block dictionaries** (`#{ … }#`). Inline-nested config without inventing
  another `[a.b.c.headers]` section every time you go one level deeper.
- **Annotations** (`@required`, `@type: int`, `@min`, `@enum: [debug, info]`,
  `@pattern: "regex"`, `@deprecated("use new_key")`). The parser validates
  them at load time and the editor flags violations as you type.
- **Variables** (`[vars]` block, `$name`, `$ENV.X ?? fallback`, arithmetic
  like `$base_url + "/v1"` or `$timeout * 2`). Stops the copy-paste-the-
  same-API-URL-in-six-places pattern.

## Install

Pick your runtime.

```bash
pip install tomlplus                              # Python 3.8+
npm install tomlplus                              # Node 18+ (native)
npm install tomlplus-wasm                         # Browser / Deno / Bun / Workers
cargo add tomlplus-syntax                         # Rust (embed the parser)
gem install tomlplus                              # Ruby
dotnet add package Tomlplus                       # .NET 8
go get github.com/CarsonKopec/tomlplus/bindings/tomlplus-go
```

Java (Gradle):

```kotlin
implementation("io.github.carsonkopec:tomlplus-java:2.0.0")
```

CLI and language server:

```bash
cargo install tomlpr            # parse / validate / fmt / vars
cargo install tomlplus-lsp      # LSP server for editors
```

C / C++ / Swift / anything with a C FFI: download
`tomlplus-ffi-<arch>.zip` from [Releases](https://github.com/CarsonKopec/tomlplus/releases).
It's a `.dll`/`.so`/`.dylib` plus the `tomlplus.h` header.

## Editor support

**VSCodium / Cursor / Windsurf / Theia / Gitpod** — install **TOML+** from
[Open VSX](https://open-vsx.org/extension/CarsonKopec/tomlplus). You get
syntax highlighting, semantic tokens, hover with resolved values,
completion, live diagnostics, inlay hints showing what `$ENV.X` actually
expands to, goto-def on `$vars`, formatting, and a colour picker on
`#RRGGBB` strings.

**Microsoft VS Code** — for now, grab the `.vsix` from a release and
`code --install-extension tomlplus-<ver>.vsix`. Marketplace listing
pending.

**Neovim / Helix / Zed / Sublime / IntelliJ / Emacs** — any LSP client
works. Point it at `tomlplus-lsp` (the binary `cargo install` just put on
your PATH), language id `tomlplus`, file extension `.tomlp`. Neovim:

```lua
vim.lsp.start({
    name = "tomlplus",
    cmd  = { "tomlplus-lsp" },
    root_dir = vim.fs.dirname(vim.fs.find({ ".git" }, { upward = true })[1]),
})
```

## How it stays consistent

There's one Rust core (`crates/tomlplus-syntax`) and every binding talks to
it. Python/Node/WASM use it directly via PyO3 / napi-rs / wasm-bindgen.
Go/Ruby/Java/.NET go through a C ABI shim (`tomlplus-ffi`) so they can
load the same `.dll`/`.so`/`.dylib` Rust produces.

On every push, CI parses the same canonical `.tomlp` fixture with all
eight bindings and JSON-compares the outputs. If a binding drifts even
by one digit, the build fails. So when you read a `.tomlp` file in Go,
it parses to the same tree it would in Python.

## Layout

```
crates/                       Rust crates (Rust-direct bindings)
  tomlplus-syntax/            the language: lexer + parser + validator + dumper
  tomlplus-lsp/               tower-lsp language server
  tomlplus-cli/               tomlpr command-line tool
  tomlplus-ffi/               C ABI cdylib + tomlplus.h
  tomlplus-python/            PyO3 wheel
  tomlplus-node/              napi-rs native module
  tomlplus-wasm/              wasm-pack module
bindings/                     non-Rust bindings (consume tomlplus-ffi)
  tomlplus-go/                cgo
  tomlplus-ruby/              FFI gem
  tomlplus-java/              Gradle, published io.github.carsonkopec:tomlplus-java
  tomlplus-dotnet/            P/Invoke
editors/vscode/               TextMate grammar + LSP client (esbuild-bundled)
tests/cross-binding/          one fixture parsed by every binding, JSON-compared
release.py                    build/test/package/publish dispatcher
```

## Develop

You need rustup. Everything else (Python venv, Gradle, MinGW for cgo on
Windows) the dispatcher fetches the first time it needs it.

```pwsh
py -3 release.py test                 # everything, all platforms (~5 min)
py -3 release.py test -t python       # one binding
py -3 release.py package -t all       # build artefacts in release/
py -3 release.py publish -t all --dry-run
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to add a binding or change
the language, and [PUBLISHING.md](PUBLISHING.md) for the per-registry
token setup.

## Status

`2.0.0-rc.x` series. CI is green across Linux / Windows / macOS-arm64 for
every binding. The macOS Intel runner got retired by GitHub, so Intel-Mac
users install from source for now. Registry pushes (PyPI, npm, crates.io,
NuGet, RubyGems, Maven Central, Open VSX) wait on one-time account setup;
see [PUBLISHING.md](PUBLISHING.md).

## License

[MIT](LICENSE). Use it however.
