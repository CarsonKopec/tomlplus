# Publishing TOML+ artifacts

The Rust core (`tomlplus-syntax`) lives in this workspace; every other
artifact is a thin wrapper or distribution channel for it. Each row below
is *independent* — you can publish one without touching the others.

| Artifact | Crate / dir | Distributes to | Audience |
| --- | --- | --- | --- |
| `tomlplus` (Python wheel) | `crates/tomlplus-python` | PyPI | `pip install tomlplus` |
| `tomlplus` (Node module)  | `crates/tomlplus-node`   | npm  | `npm install tomlplus` |
| `tomlplus-wasm` (WebAssembly) | `crates/tomlplus-wasm` | npm | browser / Deno / Bun / Cloudflare Workers |
| `tomlplus-go` (Go module) | `bindings/tomlplus-go`   | git tag → `go get` | Go devs |
| `tomlplus` (Ruby gem)     | `bindings/tomlplus-ruby` | RubyGems | `gem install tomlplus` |
| `tomlplus` (Java jar)     | `bindings/tomlplus-java` | any Maven repo | `com.tomlplus:tomlplus` |
| `Tomlplus` (NuGet pkg)    | `bindings/tomlplus-dotnet` | any NuGet feed | `Install-Package Tomlplus` |
| C library (`.dll` / `.so` / `.dylib` + header) | `crates/tomlplus-ffi` | GitHub Release tarball | C / C++ / Swift / any C-FFI host |
| `tomlplus-lsp` binary | `crates/tomlplus-lsp` | GitHub Release / `cargo install` | LSP-aware editors |
| `tomlpr` CLI | `crates/tomlplus-cli` | GitHub Release / `cargo install` | Anyone with a shell |
| `tomlplus` VS Code extension | `editors/vscode` | Marketplace + Open VSX | VS Code / Cursor / VSCodium |
| `tomlplus-syntax` crate (library) | `crates/tomlplus-syntax` | crates.io | Rust devs embedding the parser |

A new release usually means bumping the workspace `version` (in
`Cargo.toml`) and the Python / Node / VS Code `version` fields,
tagging `vX.Y.Z`, and letting CI fan-out to all of these. The
`release.py` dispatcher automates every step — manual commands per
target follow as documentation.

---

## 0. Pre-flight

```pwsh
cd C:\Users\kopec\Development\tomlplus\lsp

# All tests green via the dispatcher:
py -3 release.py test                 # everything
py -3 release.py test -t python       # one target

# Or run them by hand:
cargo test --workspace
py -3 release.py test -t python
py -3 release.py test -t node
```

---

## 1. Python — PyPI (`pip install tomlplus`)

Built with **maturin**. The wheel ships compiled Rust + a thin Python wrapper.

```pwsh
cd crates\tomlplus-python

# One-shot, current platform only:
maturin build --release
# Wheels land in `target/wheels/tomlplus-2.0.0-cp38-abi3-win_amd64.whl`

# Or via the dispatcher (handles venv, all targets):
py -3 release.py publish -t python --dry-run   # rehearsal
py -3 release.py publish -t python             # uploads to PyPI
```

For real distribution you want wheels for at least:
linux-x86_64, linux-aarch64 (manylinux), macos-x86_64, macos-arm64, win-amd64.
Use [`cibuildwheel`](https://cibuildwheel.readthedocs.io) or `maturin publish`
from a GitHub Action matrix; the abi3 build means **one wheel per OS/arch
covers Python 3.8+** — you do not need a wheel per Python minor version.

### Versioning
Update `crates/tomlplus-python/pyproject.toml::project.version`. The Python
wrapper exposes `tomlplus.__version__` from `python/tomlplus/__init__.py` —
keep them in sync (the dispatcher's `version` action does this atomically).

---

## 2. Node — npm (`npm install tomlplus`)

Built with **`@napi-rs/cli`**. The npm package ships per-platform native modules.

```pwsh
cd crates\tomlplus-node
npx napi build --platform --release       # produces tomlplus.<triple>.node
node --test test.mjs

# Or via the dispatcher:
py -3 release.py publish -t node --dry-run
py -3 release.py publish -t node
```

`napi-rs` produces a meta package that depends on `optionalDependencies` for
each platform sub-package. The right `.node` file is auto-selected at install.

---

## 3. C library — GitHub Release

For Go / Ruby / Java / .NET / C / C++ / Swift — anyone with a C FFI.

```pwsh
py -3 release.py package -t ffi
# Stages release/tomlplus-ffi-<ver>-windows-x86_64/ + .zip with:
#   tomlplus_ffi.dll, tomlplus_ffi.dll.lib, tomlplus_ffi.lib, tomlplus.h
```

Run the same step on Linux + macOS runners in CI to produce
`tomlplus-ffi-<ver>-linux-x86_64.zip` etc.; the dispatcher's
`publish -t all` action calls `gh release create` and uploads everything
from `release/`.

---

## 4. LSP server + `tomlpr` CLI — `cargo install` + GitHub Release

```pwsh
py -3 release.py package -t lsp        # → release/tomlplus-lsp.exe
py -3 release.py package -t cli        # → release/tomlpr.exe

# Or directly:
cargo install --path crates/tomlplus-lsp
cargo install --path crates/tomlplus-cli
```

The dispatcher's `publish -t rust` calls `cargo publish` in dependency order
(syntax → ffi → cli → lsp) with a 10s sleep between to let crates.io index.

---

## 5. VS Code extension — Marketplace + Open VSX

```pwsh
py -3 release.py package -t vscode     # produces .vsix
py -3 release.py publish -t vscode     # pushes to Marketplace (+ Open VSX if OVSX_PAT set)
```

Keep `editors/vscode/package.json::version` aligned with the workspace
version (the `version` action does this).

---

## 6. Rust library — crates.io

```pwsh
py -3 release.py publish -t rust            # publishes syntax → ffi → cli → lsp
py -3 release.py publish -t rust --dry-run  # rehearsal
```

You need a crates.io API token in `CARGO_REGISTRY_TOKEN` (or `cargo login`
once interactively).

---

## 7. Java — Maven Central via Sonatype Central Portal

Source under `bindings/tomlplus-java`. **Gradle** build (Kotlin DSL) using
the [Vanniktech Maven Publish plugin](https://vanniktech.github.io/gradle-maven-publish-plugin/),
which targets the new Sonatype **Central Portal** (`central.sonatype.com`).
Bootstraps a local Gradle distribution into `.gradle-local/` so no system
install is needed.

**Coordinates**: `io.github.carsonkopec:tomlplus-java:2.0.0`

```pwsh
py -3 release.py test    -t java       # JUnit 5 via Gradle
py -3 release.py package -t java       # build jars + stage Maven layout
py -3 release.py publish -t java       # upload to Central Portal
```

### One-time setup: namespace + tokens + GPG key

1. **Claim the `io.github.carsonkopec` namespace.**
   * Sign in to <https://central.sonatype.com/> with your GitHub account.
   * **Namespaces → Add Namespace**, enter `io.github.carsonkopec`.
   * Portal asks you to create a public GitHub repo named `OSSRH-XXXXXX`
     in your account; create the empty repo and click **Verify**. Approval
     is instant.

2. **Generate a user token.**
   * Top-right menu → **View Account → Generate User Token**.
   * Copy the two strings — that's your `CENTRAL_USERNAME` and
     `CENTRAL_PASSWORD`.

3. **Create a GPG signing key.** Central rejects unsigned artefacts.

   ```pwsh
   winget install GnuPG.Gpg4win
   gpg --full-generate-key                           # RSA 4096, no expiry
   gpg --list-secret-keys --keyid-format=long        # note the long key ID
   gpg --keyserver keys.openpgp.org --send-keys <KEYID>
   gpg --armor --export-secret-keys <KEYID> > secrets\gpg-private.asc
   ```

### Required env vars / GitHub Actions secrets

| Name | Value |
| --- | --- |
| `CENTRAL_USERNAME` | user-token *username* string from step 2 |
| `CENTRAL_PASSWORD` | user-token *password* string from step 2 |
| `SIGN_KEY`         | contents of `secrets\gpg-private.asc` (whole armoured block) |
| `SIGN_PASSWORD`    | the passphrase you set when generating the key |

The dispatcher translates these into the
`ORG_GRADLE_PROJECT_mavenCentralUsername` / `signingInMemoryKey`-style
properties that the Vanniktech plugin reads.

### Local rehearsal

```pwsh
# Stage a full Maven publication to ~/.m2 (no creds needed):
py -3 release.py package -t java

# Inspect — the layout should mirror exactly what Central will accept:
ls $HOME\.m2\repository\io\github\carsonkopec\tomlplus-java\2.0.0\
# Expect: .jar, -sources.jar, -javadoc.jar, .pom, .module, plus matching
# .asc signature files when SIGN_KEY is set.
```

### Real publish

```pwsh
$env:CENTRAL_USERNAME = '<central-user-token>'
$env:CENTRAL_PASSWORD = '<central-user-token-password>'
$env:SIGN_KEY         = (Get-Content secrets\gpg-private.asc -Raw)
$env:SIGN_PASSWORD    = '<gpg-passphrase>'

py -3 release.py publish -t java
# Runs `gradle publishAndReleaseToMavenCentral` under the hood — uploads
# the bundle and immediately promotes it.
```

If you'd rather **inspect in the Central UI before promoting**, swap the
task in `release.py::publish_java` from `publishAndReleaseToMavenCentral`
→ `publishToMavenCentral`. The staged bundle shows up under
<https://central.sonatype.com/publishing/deployments> where you can review
and click *Publish* manually.

### Consuming `io.github.carsonkopec:tomlplus-java`

```kotlin
// build.gradle.kts
dependencies {
    implementation("io.github.carsonkopec:tomlplus-java:2.0.0")
}
```

```xml
<!-- pom.xml -->
<dependency>
    <groupId>io.github.carsonkopec</groupId>
    <artifactId>tomlplus-java</artifactId>
    <version>2.0.0</version>
</dependency>
```

---

## 8. .NET — any NuGet feed

Source under `bindings/tomlplus-dotnet`. .NET 8 class library; uses P/Invoke.

```pwsh
py -3 release.py test    -t dotnet     # xUnit
py -3 release.py package -t dotnet     # produces Tomlplus.2.0.0.nupkg
py -3 release.py publish -t dotnet     # pushes to NUGET_FEED_URL (default nuget.org)
```

### Publishing

| Target | `NUGET_FEED_URL` |
| --- | --- |
| **nuget.org** (default) | `https://api.nuget.org/v3/index.json` |
| **GitHub Packages** | `https://nuget.pkg.github.com/<owner>/index.json` |
| **Azure Artifacts** | `https://pkgs.dev.azure.com/<org>/_packaging/<feed>/nuget/v3/index.json` |
| **Local folder** | `C:\nuget-cache` |

```pwsh
$env:NUGET_API_KEY  = '<token>'
$env:NUGET_FEED_URL = 'https://nuget.pkg.github.com/CarsonKopec/index.json'
py -3 release.py publish -t dotnet
```

The shared `tomlplus_ffi.{dll,so,dylib}` library must be on the runtime's
native loader search path. For zero-install consumers, you can ship it as
a NuGet `runtimes/<rid>/native/` asset.

---

## 9. Go — Go modules (`go get`)

Source under `bindings/tomlplus-go`. Distributes by git tag — the
toolchain just clones the tag from GitHub.

```pwsh
py -3 release.py test -t go            # builds FFI, runs go test ./...
```

Publish:

```pwsh
# 1. Tag the workspace (the release action does this for you):
git tag v2.0.0 && git push --tags
# 2. Consumers then:
go get github.com/CarsonKopec/tomlplus/bindings/tomlplus-go@v2.0.0
```

Users install `tomlplus_ffi.{dll,so,dylib}` themselves — Go modules
don't ship binaries.

On Windows the cgo build needs `gcc` (RubyInstaller's DevKit / MSYS2 /
TDM-GCC). The dispatcher auto-detects.

---

## 10. Ruby — RubyGems (`gem install tomlplus`)

Source under `bindings/tomlplus-ruby`. Pure-Ruby gem using `ffi`.

```pwsh
py -3 release.py test    -t ruby
py -3 release.py package -t ruby
py -3 release.py publish -t ruby       # gem push
```

The gem reads `TOMLPLUS_LIB` (a path to the shared library) before its
default search names; the dispatcher sets it automatically. On Windows
this works around `LoadLibrary` ignoring `%PATH%`.

---

## 11. WASM — `tomlplus-wasm` on npm

Single artefact for every JS runtime that isn't Node. Source under
`crates/tomlplus-wasm`. Built with **wasm-pack** — no FFI involved; the
WASM module embeds `tomlplus-syntax` directly.

```pwsh
py -3 release.py build   -t wasm       # builds pkg-web, pkg-nodejs, pkg-bundler
py -3 release.py test    -t wasm       # node --test against pkg-node
py -3 release.py publish -t wasm       # npm publish the bundler variant
```

| Consumer | wasm-pack `--target` | Import style |
| --- | --- | --- |
| Browser, plain ES modules | `web` | `import init, { parse } from "tomlplus-wasm"; await init();` |
| Webpack / Rollup / Vite / esbuild | `bundler` | `import { parse } from "tomlplus-wasm";` (auto-loads) |
| Node.js (CommonJS) | `nodejs` | `const { parse } = require("tomlplus-wasm");` |
| Deno / Bun / inline `<script>` | `no-modules` | `<script src="tomlplus-wasm.js"></script>` |

The Rust core's filesystem / process / hostname builtins (`$CWD`, `$PID`,
`$HOSTNAME`) return empty strings / `0` under WASM since the host has no
OS to query; every other feature is identical to the native bindings.

---

## 12. Recommended release flow

```pwsh
# One command bumps every manifest, commits, tags, packages, pushes,
# and runs `gh release create`:
py -3 release.py release --new 2.1.0 --dry-run   # rehearsal first
py -3 release.py release --new 2.1.0
```

What it does, in order:
1. Bump `version` in 5 manifests (workspace Cargo.toml, pyproject.toml,
   tomlplus/__init__.py, two package.json's).
2. `cargo update --workspace` to refresh Cargo.lock.
3. `git add -A && git commit && git tag vX.Y.Z`.
4. Build + test + package everything.
5. Push to crates.io, PyPI, npm, NuGet, RubyGems, Maven, Marketplace,
   GitHub Packages, GitHub Release — whichever creds are present in env.
6. `git push --tags`.

Missing tokens degrade gracefully — the dispatcher skips that one publish
step and continues.

---

## 13. CI / CD (GitHub Actions)

Two workflows at the repo root in `.github/workflows/`:

| File | Triggers on | What it does |
| --- | --- | --- |
| `ci.yml` | every push + PR | rustfmt + clippy, then a matrix `{ ubuntu, windows, macos } × { rust, python, node, wasm, java, dotnet, go, ruby }` runs `python release.py test -t <target>`. Plus a dedicated FFI loader job for Java + .NET on all three OSes. |
| `release.yml` | tag push `v*.*.*` | matrix `{ ubuntu, windows, macos-13, macos-14 } → python release.py package -t all` uploading each `release/` to GitHub Actions storage. Then a single Linux `publish` job downloads them all into a merged `release/` and runs `python release.py publish -t all`. Finally `gh release create` attaches every artefact to a GitHub Release. |

### Required GitHub Actions secrets

Set these under **Settings → Secrets and variables → Actions**:

| Secret | Used by |
| --- | --- |
| `PYPI_TOKEN`           | `release.py publish -t python` (twine) |
| `NPM_TOKEN`            | npm publish (Node + WASM) |
| `CARGO_REGISTRY_TOKEN` | crates.io |
| `NUGET_API_KEY`        | nuget.org or any configured feed |
| `NUGET_FEED_URL`       | *(optional)* override default nuget.org |
| `MAVEN_REPO_URL`       | Maven Central / GitHub Packages / Nexus |
| `CENTRAL_USERNAME`  | Maven repo auth |
| `CENTRAL_PASSWORD`  | Maven repo auth |
| `SIGN_KEY`             | *(Maven Central only)* GPG private key |
| `SIGN_PASSWORD`        | GPG key passphrase |
| `VSCE_PAT`             | VS Code Marketplace |
| `OVSX_PAT`             | *(optional)* Open VSX |
| `RUBYGEMS_API_KEY`     | rubygems.org |
| `GITHUB_TOKEN`         | automatic, no setup needed |

Missing secrets degrade gracefully — the dispatcher logs a `[--] DryRun-like`
message and continues with the rest of the pipeline.

### Local-equivalent commands

Every CI step is reachable locally:

```pwsh
py -3 release.py test -t all          # what ci.yml runs
py -3 release.py package -t all       # what release.yml's `package` step runs
py -3 release.py publish -t all       # what release.yml's `publish` step runs
py -3 release.py release --new 2.1.0  # what a tag push triggers end-to-end
```

The workflows just call this script with the right env, so debugging CI
failures usually means reproducing locally with the same `-t <target>`.

---

## 14. Future bindings

Same `bindings/tomlplus-<lang>` (or `crates/tomlplus-<lang>` for Rust-native
ABIs) pattern applies to **Swift**, **Kotlin** (Android), **PHP** (built-in
FFI in 7.4+), **Lua**, **OCaml**, etc. Each reuses `tomlplus-ffi` (or
`tomlplus-syntax` directly for ABIs that talk to Rust without a C layer:
PyO3, napi-rs, wasm-pack). All bindings derive from a single Rust core, so
behaviour can't drift.
