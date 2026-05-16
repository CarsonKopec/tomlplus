# Contributing to TOML+

Thanks for considering a contribution. This doc covers the bits of the
project that aren't obvious from reading the code: where things live, how
to run the tests, and what to do when you change the language itself.

## Repo at a glance

* **The language lives in [`crates/tomlplus-syntax`](crates/tomlplus-syntax)**
  (lexer + parser + validator + dumper). Pure Rust library, no I/O, no
  async, no LSP types. Every other crate consumes it.
* **Bindings** come in two flavours:
  * `crates/tomlplus-<lang>/` for ABIs that talk to Rust directly: Python
    (PyO3), Node (napi-rs), WASM (wasm-bindgen).
  * `bindings/tomlplus-<lang>/` for ABIs that go through the C FFI in
    `crates/tomlplus-ffi`: Go (cgo), Ruby (`ffi` gem), Java (JNA),
    .NET (P/Invoke).
* **Tooling**: `release.py` orchestrates everything. CI is in
  `.github/workflows/`.
* **Cross-binding correctness**: one fixture in
  [`tests/cross-binding/fixtures/kitchen-sink.tomlp`](tests/cross-binding/fixtures/kitchen-sink.tomlp)
  is parsed by every binding and compared to a canonical JSON baseline.
  This is the single biggest guard against bindings drifting from each
  other.

## Dev workflow

```pwsh
# One-time setup:
winget install Rustlang.Rustup       # or https://rustup.rs/

# The dispatcher does everything:
py -3 release.py build                # build everything
py -3 release.py test                 # run every test (incl. cross-binding)
py -3 release.py test -t python       # one target
py -3 release.py package -t all       # produce shippable artefacts
py -3 release.py publish -t all --dry-run

# Common shortcuts:
cargo test --workspace                # Rust unit tests only (fast)
cargo build -p tomlplus-lsp --release # one crate at a time
```

`release.py` auto-installs `rich`, bootstraps a local Gradle, and detects a
MinGW gcc on Windows. The first run takes a while; subsequent runs are
incremental.

## When you change the language

If your change affects the *parsed output* of any TOML+ file:

1. Update the parser in `crates/tomlplus-syntax/`.
2. Add a unit test to the relevant `mod tests` (lexer, value_parser,
   parser, validator, dumper).
3. Run `cargo test --workspace`.
4. **Regenerate the cross-binding baseline**: changes that should affect
   every binding's output need a new `expected.json`:

   ```pwsh
   py -3 release.py build -t cli
   target\release\tomlpr.exe parse tests\cross-binding\fixtures\kitchen-sink.tomlp `
       > tests\cross-binding\fixtures\expected.json
   py -3 release.py test -t cross-binding   # verify all 8 bindings agree
   ```

5. If a *new* feature is being added (e.g. a new annotation, a new
   numeric literal form, a new builtin), extend
   `tests/cross-binding/fixtures/kitchen-sink.tomlp` to exercise it.

## When you add a new binding

Pattern:

1. New crate under `crates/tomlplus-<lang>/` (Rust-native ABI) or
   `bindings/tomlplus-<lang>/` (C-FFI consumer).
2. Mirror the Python/Node public API where it makes sense in the host
   language.
3. Add a harness in `tests/cross-binding/harness/<lang>` that reads a
   `.tomlp` file from argv[1] and prints the config as JSON on stdout.
4. Register the harness in `discover_harnesses()` in
   `tests/cross-binding/run.py`.
5. Add the binding to `release.py`'s `ALL_TARGETS` plus a `Build-X`,
   `Test-X`, `Package-X`, `Publish-X` (if there's a registry).
6. Document in `PUBLISHING.md` (which registry, which env vars).
7. Add the toolchain to `.github/workflows/ci.yml` (test matrix + the
   cross-binding job's setup steps).

## When you change the LSP

The LSP server is at `crates/tomlplus-lsp/`. The VS Code client at
`editors/vscode/` is the canonical reference editor.

To test changes locally:

```pwsh
cargo install --path crates/tomlplus-lsp --force
# Restart the EDH (F5 in the editors/vscode workspace) — the extension
# picks up the new binary from ~/.cargo/bin.
```

For features that touch the parser too (e.g. adding spans, surfacing new
errors), the LSP's behaviour ultimately depends on what
`tomlplus-syntax::Document` exposes. Add fields there first.

## Style

* Rust: `cargo fmt --all` before pushing. CI enforces `cargo clippy
  --workspace --all-targets -- -D warnings`.
* Python (`release.py`, harnesses, orchestrator): no external linter
  configured; keep functions short and the rich output consistent with
  the existing style (`step()` for phase headers, `ok` / `warn` /
  `skip` helpers, never `print()`).
* TypeScript (VS Code client): `npm run compile` with `strict: true`.

## Releases

See [`PUBLISHING.md`](PUBLISHING.md) for the full per-target release
commands. The TL;DR for the maintainer is:

```pwsh
py -3 release.py release --new 2.X.Y --dry-run   # rehearsal
py -3 release.py release --new 2.X.Y             # the real thing
```

…which bumps every manifest, commits, tags, builds, tests, packages, and
publishes to every registry whose secret is configured in CI.

## Questions

Open an issue. Even half-formed "I think feature X is wrong" reports are
useful — they're the cheapest way to lock down the language's shape
before users have written too many `.tomlp` files we can't break.
