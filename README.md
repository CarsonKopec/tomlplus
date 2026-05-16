# TOML+

> TOML, plus block dictionaries, annotations, and variables.

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

| Language | Install | Status |
| --- | --- | --- |
| Python | `pip install tomlplus` | ![](https://img.shields.io/pypi/v/tomlplus.svg) |
| Node / Deno / Bun | `npm install tomlplus` (native) · `npm install tomlplus-wasm` (universal) | ![](https://img.shields.io/npm/v/tomlplus.svg) |
| Rust | `cargo add tomlplus-syntax` | ![](https://img.shields.io/crates/v/tomlplus-syntax.svg) |
| Java | `com.tomlplus:tomlplus` on Maven Central | — |
| .NET | `dotnet add package Tomlplus` | — |
| Go | `go get github.com/CarsonKopec/tomlplus/bindings/tomlplus-go` | — |
| Ruby | `gem install tomlplus` | — |
| C / C++ / others | Download `tomlplus-ffi-<ver>-<arch>.zip` from [Releases](https://github.com/CarsonKopec/tomlplus/releases) | — |

CLI:

```bash
cargo install tomlpr               # parse / validate / fmt / vars
cargo install tomlplus-lsp         # language server for editors
```

VS Code: install the **TOML+** extension from the Marketplace.

## Editor support

* **VS Code / Cursor / VSCodium**: extension at [editors/vscode](editors/vscode/) → ships syntax highlighting, semantic tokens, hover, completion, diagnostics, inlay hints, goto-def, formatting, colour picker for `#RRGGBB` strings.
* **Neovim / Helix / Zed / Sublime / IntelliJ / Emacs**: point any LSP client at the `tomlplus-lsp` binary, language id `tomlplus`, file extension `.tomlp`. Neovim example:

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
├── release.py                     # build/test/package/publish dispatcher
├── PUBLISHING.md                  # per-target release commands
├── CHANGELOG.md
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
│   ├── tomlplus-java/             # JNA Gradle artefact
│   └── tomlplus-dotnet/           # P/Invoke .NET class library
├── editors/vscode/                # VS Code client extension
└── tests/cross-binding/           # one fixture parsed by every binding, JSON-compared
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
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the dev workflow and
[`PUBLISHING.md`](PUBLISHING.md) for per-registry publish commands.

## License

[MIT](LICENSE) © Carson Kopec.
