# Changelog

All notable changes to TOML+ live here. Versions follow [SemVer](https://semver.org/).

The version refers to the **`tomlplus-syntax`** core; every binding tracks
the same number with very rare per-binding hotfix patches (e.g. `2.0.1` of
the Python wheel without bumping the Rust core would get a `+py.1` suffix).

## [Unreleased]

Nothing yet — the next release will collect items here.

## [2.0.0] — 2026-XX-XX (pre-release)

The first release of the Rust-rewritten language. Replaces the pure-Python
`tomlplus` 1.x reference implementation with a Rust core (`tomlplus-syntax`)
that every binding shares.

### Added — language

* Block dictionaries: `key = #{ … }#` for inline + multi-line nested
  config.
* Inline dictionaries on a single line: `colors = #{ a = 1, b = 2 }#`.
* Annotations: `@required`, `@type: T`, `@min: N`, `@max: N`, `@minlen`,
  `@maxlen`, `@pattern: "regex"`, `@enum: […]`, `@positive`, `@nonzero`,
  `@nonempty`, `@deprecated("msg")`, `@tag: k = "v"`, plus metadata-only
  flags `@internal`, `@readonly`, `@experimental`.
* Variables: `[vars]` section, `$name` references, `$ENV.VAR ?? fallback`,
  arithmetic (`$x + 1`, `$base + "/api"`).
* Built-in variables: `$NOW`, `$TODAY`, `$TRUE`, `$FALSE`, `$NULL`,
  `$PID`, `$HOSTNAME`, `$PLATFORM`, `$CWD`.
* Dotted / nested sections: `[server.cors]` → `{server: {cors: {…}}}`.
* Quoted keys: `"my key" = "value"`, `"a.b" = "x"`.
* Numeric literals: hex `0xff`, oct `0o755`, binary `0b1010`,
  underscores `1_000_000`, scientific `1.5e3`, negative literals.
* Multi-line arrays and multi-line inline dicts; trailing commas allowed
  in block dicts.

### Added — implementations

* **`tomlplus-syntax`** — pure Rust library: lexer, parser, validator,
  dumper. No I/O, no async, no LSP types. Owns the language definition.
* **`tomlplus-lsp`** — `tower-lsp` language server (LSP over stdio).
  Capabilities: full-document diagnostics (parse + validate), hover
  (markdown with resolved-value / annotation table), completion (`@`/`$`
  triggered, snippet-based, per-item docs), document symbols (nested
  sections), goto-definition for `$user_var`, formatting (round-trips
  through the dumper), semantic tokens (11 token types: `namespace`,
  `property`, `decorator`, `type`, `enumMember`, `variable`, `constant`,
  `parameter`, `number`, `string`, `regexp` + 5 modifiers), inlay hints
  (`→ "value"` for resolved env vars + user vars, `: int` for `@type:`
  annotated keys), color provider (`#RRGGBB` strings → in-editor colour
  picker).
* **`tomlplus-cli`** (`tomlpr`) — `parse`, `validate`, `fmt`, `vars`
  subcommands.
* **`tomlplus-ffi`** — C ABI cdylib + `tomlplus.h` header. Single shared
  artefact consumed by every non-Rust binding.
* **`tomlplus-python`** — PyO3 wheel, `abi3-py38` so one wheel per
  OS/arch covers Python 3.8+. Drop-in API replacement for the pure-Python
  `tomlplus` 1.x package (passes all 133 of its tests unmodified).
* **`tomlplus-node`** — napi-rs native module, multi-arch via
  `napi prepublish`.
* **`tomlplus-wasm`** — `wasm-pack` build for `web` / `nodejs` /
  `bundler` / `no-modules` targets. Runs in browsers, Deno, Bun,
  Cloudflare Workers.
* **`tomlplus-java`** — Gradle build, JNA over the C FFI. Published to
  Maven Central via the Sonatype Central Portal under
  `io.github.carsonkopec:tomlplus-java`. Uses the Vanniktech Maven Publish
  plugin for upload + GPG signing.
* **`tomlplus-dotnet`** — .NET 8 class library, P/Invoke over the C FFI,
  configurable NuGet feed URL.
* **`tomlplus-go`** — cgo wrapper. On Windows, links against the DLL
  directly via `-l:tomlplus_ffi.dll` to side-step the MSVC/MinGW
  staticlib mismatch.
* **`tomlplus-ruby`** — FFI gem; respects `TOMLPLUS_LIB` env to point at
  the shared library on Windows where `LoadLibrary` ignores `%PATH%`.
* **VS Code extension** — TypeScript client at `editors/vscode/`,
  TextMate grammar, language config, full LSP capability set.

### Added — tooling

* `release.py` — single Python+`rich` dispatcher for the whole pipeline
  (build / test / package / publish / version / release). Cross-platform
  (Windows + Linux + macOS). Auto-installs `rich`, bootstraps a local
  Gradle if needed, auto-detects MinGW gcc for cgo on Windows.
* GitHub Actions workflows: `ci.yml` (rustfmt + clippy + per-binding
  test matrix across Ubuntu/Windows/macOS + cross-binding correctness
  test) and `release.yml` (per-OS package build → single publish job
  that pushes to every registry whose secret is configured).
* `tests/cross-binding/` — one canonical `kitchen-sink.tomlp` fixture
  parsed by every binding (Python, Node, WASM, Java, .NET, Go, Ruby,
  plus the Rust CLI as the canonical reference). Outputs are
  JSON-compared via a normalised deep-diff (numeric tolerance + key
  ordering ignored). All 8 bindings produce byte-identical output as
  of 2.0.0.

### Changed (from pre-release / 1.x)

* Inline-dict closer unified from `}` to `}#` so both inline and block
  dicts use the same delimiter pair.
* `dumper`: block-dict open is now `#{` (was previously emitted as `{#`
  in 1.x — a real bug, never caught by tests).
* Arithmetic now works inside arrays and inline dicts (`_array` /
  `_inline_dict` call the expression parser instead of the atom parser).
* Section-level annotations: leaf-only annotations (`@type`, `@min`,
  …) are silently skipped on section dicts instead of producing
  spurious validation errors. `@tag`, `@deprecated`, `@required` still
  apply.

### Known gaps (planned for 2.1)

* Multi-line strings (`"""…"""`).
* RFC 3339 datetime literals.
* `\uXXXX` Unicode escapes in strings.
* LSP code actions (auto-quote unsafe keys, replace deprecated keys,
  extract value to `[vars]`).
* LSP rename + find-references for `$variables`.
* Shared `.tomlp.schema` files for cross-service validation.
