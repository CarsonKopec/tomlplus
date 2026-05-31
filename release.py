#!/usr/bin/env python3
"""
TOML+ release automation.

  python release.py build    [-t TARGET]
  python release.py test     [-t TARGET]
  python release.py package  [-t TARGET]
  python release.py publish  [-t TARGET] [--dry-run]
  python release.py version  --new X.Y.Z
  python release.py clean
  python release.py release  --new X.Y.Z [--dry-run]

Targets:
    syntax  ffi  cli  lsp  python  node  wasm  vscode
    go  ruby  java  dotnet
    rust         (= syntax + ffi + cli + lsp)
    c-bindings   (= ffi + go + ruby + java + dotnet)
    all          (= everything)

Cross-platform (Windows / Linux / macOS); shells out to whichever toolchains
are on PATH. See PUBLISHING.md for the full per-target details.

Required env vars per publish target:
    python   PYPI_TOKEN
    node     NPM_TOKEN
    crates   CARGO_REGISTRY_TOKEN
    vscode   VSCE_PAT, optional OVSX_PAT
    dotnet   NUGET_API_KEY, optional NUGET_FEED_URL
    ruby     RUBYGEMS_API_KEY (or `gem signin`)
    java     CENTRAL_USERNAME, CENTRAL_PASSWORD (Sonatype Central user token)
             SIGN_KEY, SIGN_PASSWORD            (ASCII-armoured GPG private key + passphrase)
    release  GITHUB_TOKEN
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import stat
import subprocess
import sys
import time
import urllib.request
import zipfile
from pathlib import Path
from typing import Callable, Iterable

# Force UTF-8 console on Windows so rich's box-drawing / check-mark glyphs
# don't crash cp1252 stdout.
for stream in (sys.stdout, sys.stderr):
    try:
        stream.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, ValueError):
        pass

# ── Bootstrap `rich` ─────────────────────────────────────────────────────────
try:
    from rich.console import Console
    from rich.panel import Panel
    from rich.table import Table
    from rich.text import Text
    from rich.theme import Theme
except ImportError:
    print("Installing 'rich' …")
    subprocess.run([sys.executable, "-m", "pip", "install", "--quiet", "rich"], check=True)
    from rich.console import Console
    from rich.panel import Panel
    from rich.table import Table
    from rich.text import Text
    from rich.theme import Theme

# ── Console + styles ─────────────────────────────────────────────────────────
THEME = Theme({
    "step":    "bold cyan",
    "cmd":     "dim italic",
    "ok":      "bold green",
    "warn":    "bold yellow",
    "skip":    "dim white",
    "err":     "bold red",
    "target":  "bold magenta",
    "version": "bold blue",
})
console = Console(theme=THEME, highlight=False)

def step(msg: str) -> None:
    console.print()
    console.print(f"[step]==>[/step] {msg}")

def cmd_echo(args: Iterable[str], cwd: Path | None = None) -> None:
    where = f" [dim]({cwd.name})[/dim]" if cwd else ""
    console.print(f"   [cmd]$ {' '.join(_shellquote(a) for a in args)}[/cmd]{where}")

def ok(msg: str)   -> None: console.print(f"   [ok]✓[/ok] {msg}")
def warn(msg: str) -> None: console.print(f"   [warn]![/warn] {msg}")
def skip(msg: str) -> None: console.print(f"   [skip]⊘[/skip] [skip]{msg}[/skip]")
def info(msg: str) -> None: console.print(f"   [dim]{msg}[/dim]")
def err(msg: str)  -> None: console.print(f"   [err]✗[/err] {msg}")

def _shellquote(s: str) -> str:
    return f'"{s}"' if " " in s and not (s.startswith('"') and s.endswith('"')) else s

# ── Paths ────────────────────────────────────────────────────────────────────
# The workspace is the directory containing this script — it's the Cargo
# workspace root and the git repo root.
WORKSPACE_ROOT  = Path(__file__).resolve().parent
CRATES_DIR      = WORKSPACE_ROOT / "crates"
BINDINGS_DIR    = WORKSPACE_ROOT / "bindings"
SYNTAX_DIR      = CRATES_DIR / "tomlplus-syntax"
LSP_BIN_DIR     = CRATES_DIR / "tomlplus-lsp"
CLI_DIR         = CRATES_DIR / "tomlplus-cli"
FFI_DIR         = CRATES_DIR / "tomlplus-ffi"
PY_DIR          = CRATES_DIR / "tomlplus-python"
NODE_DIR        = CRATES_DIR / "tomlplus-node"
WASM_DIR        = CRATES_DIR / "tomlplus-wasm"
VSCODE_DIR      = WORKSPACE_ROOT / "editors" / "vscode"
GO_DIR          = BINDINGS_DIR / "tomlplus-go"
RUBY_DIR        = BINDINGS_DIR / "tomlplus-ruby"
JAVA_DIR        = BINDINGS_DIR / "tomlplus-java"
DOTNET_DIR      = BINDINGS_DIR / "tomlplus-dotnet"
RELEASE_DIR     = WORKSPACE_ROOT / "release"
TARGET_DIR      = WORKSPACE_ROOT / "target"

IS_WINDOWS = os.name == "nt"
EXE = ".exe" if IS_WINDOWS else ""

# ── Subprocess wrapper ───────────────────────────────────────────────────────
class CommandError(RuntimeError):
    pass

def run(args: list[str], *, cwd: Path | None = None, env: dict[str, str] | None = None,
        check: bool = True, capture: bool = False) -> subprocess.CompletedProcess:
    """Run a command, echoing it to the console.

    On Windows we resolve `.cmd` / `.bat` shims (npm, npx, maturin, gradle,
    etc.) ourselves, since Python's CreateProcess won't apply PATHEXT.
    """
    cmd_echo(args, cwd)
    if env is None:
        env = os.environ.copy()
    # On Windows, npm / npx / maturin / gradle etc. are `.cmd` shims that
    # CreateProcess can't launch directly. Route everything that isn't a
    # plain .exe through cmd.exe via shell=True with a list2cmdline string,
    # which handles quoting paths-with-spaces correctly.
    use_shell = False
    if IS_WINDOWS and args and not _is_native_exe(args[0]):
        use_shell = True
        args = subprocess.list2cmdline(args)  # type: ignore[assignment]
    if capture:
        result = subprocess.run(args, cwd=cwd, env=env, shell=use_shell,
                                capture_output=True, text=True, encoding="utf-8")
    else:
        result = subprocess.run(args, cwd=cwd, env=env, shell=use_shell)
    if check and result.returncode != 0:
        raise CommandError(f"`{' '.join(args)}` failed with exit {result.returncode}")
    return result

def _is_native_exe(name: str) -> bool:
    """True iff `name` is something CreateProcess can launch directly:
    an absolute path to an .exe, or just an .exe basename."""
    if name.lower().endswith(".exe"):
        return True
    if os.path.isabs(name):
        return False  # absolute non-exe path → still need cmd.exe
    # Bare name: check whether the resolved file is an .exe.
    found = shutil.which(name)
    return bool(found and found.lower().endswith(".exe"))

def _resolve_windows_shim(name: str, env: dict[str, str]) -> str | None:
    """Find an executable / .cmd / .bat shim for *name* on the PATH inside *env*.

    Search order is **PATHEXT first** (so `npx.cmd` wins over the
    bare-name `npx` shell script that ships next to it), then fall back to
    the unextended file as a last resort.
    """
    if os.path.isabs(name) and Path(name).exists():
        return None  # already absolute; let CreateProcess handle it
    if Path(name).suffix:  # caller already supplied an extension
        return None
    pathext = env.get("PATHEXT", ".COM;.EXE;.BAT;.CMD")
    exts = [e for e in pathext.split(";") if e] + [""]
    for d in env.get("PATH", "").split(os.pathsep):
        if not d:
            continue
        for ext in exts:
            cand = Path(d) / (name + ext)
            if cand.is_file():
                return str(cand)
    return None

# ── Environment helpers ──────────────────────────────────────────────────────
def add_to_path(d: Path) -> None:
    sep = ";" if IS_WINDOWS else ":"
    parts = os.environ["PATH"].split(sep)
    if str(d) not in parts:
        os.environ["PATH"] = f"{d}{sep}{os.environ['PATH']}"

def cargo_path() -> None:
    add_to_path(Path.home() / ".cargo" / "bin")

def add_ffi_to_loader_path() -> None:
    """Make tomlplus_ffi.{dll,so,dylib} discoverable to runtime loaders.

    Different OSes use different env vars for shared-library lookup:
      Windows : %PATH%               (LoadLibrary)
      Linux   : $LD_LIBRARY_PATH     (ld.so / dlopen)
      macOS   : $DYLD_LIBRARY_PATH   (dyld)

    We prepend the release-build dir to whichever applies, plus PATH on
    every OS so co-located executables resolve too.
    """
    release = TARGET_DIR / "release"
    add_to_path(release)
    if not IS_WINDOWS:
        var = "DYLD_LIBRARY_PATH" if sys.platform == "darwin" else "LD_LIBRARY_PATH"
        existing = os.environ.get(var, "")
        if str(release) not in existing.split(os.pathsep):
            os.environ[var] = f"{release}{os.pathsep}{existing}" if existing else str(release)

def has_tool(name: str) -> Path | None:
    p = shutil.which(name)
    return Path(p) if p else None

def require_tool(name: str, hint: str = "") -> Path:
    p = has_tool(name)
    if not p:
        msg = f"{name} not found on PATH."
        if hint:
            msg += f" {hint}"
        raise CommandError(msg)
    return p

def env_get(*names: str) -> str | None:
    """First env var (in order) with a non-empty value."""
    for n in names:
        v = os.environ.get(n)
        if v:
            return v
    return None

def require_env(name: str, why: str) -> str:
    v = os.environ.get(name)
    if not v:
        raise CommandError(f"{name} is not set. Needed to: {why}")
    return v

# ── Python venv ──────────────────────────────────────────────────────────────
def ensure_py_venv() -> Path:
    """Returns the venv's python executable path."""
    venv = PY_DIR / ".venv"
    if not venv.exists():
        info(f"Creating Python venv at {venv}")
        run(["py", "-3", "-m", "venv", str(venv)] if IS_WINDOWS
            else [sys.executable, "-m", "venv", str(venv)])
    scripts = venv / ("Scripts" if IS_WINDOWS else "bin")
    py = scripts / ("python.exe" if IS_WINDOWS else "python")
    os.environ["VIRTUAL_ENV"] = str(venv)
    add_to_path(scripts)
    marker = venv / ".deps-installed"
    if not marker.exists():
        run([str(py), "-m", "pip", "install", "--quiet", "--upgrade", "pip"])
        run([str(py), "-m", "pip", "install", "--quiet", "maturin", "pytest", "twine"])
        marker.write_text(str(time.time()))
    return py

# ── Version management ───────────────────────────────────────────────────────
def update_version(new_version: str) -> None:
    if not re.match(r"^\d+\.\d+\.\d+(?:[-+][\w.-]+)?$", new_version):
        raise CommandError(f"Version {new_version!r} is not semver (X.Y.Z).")
    step(f"Bumping version → [version]{new_version}[/version]")

    def replace(path: Path, pattern: str, replacement: str) -> None:
        text = path.read_text(encoding="utf-8")
        new_text = re.sub(pattern, replacement, text, count=1, flags=re.MULTILINE)
        if new_text != text:
            path.write_text(new_text, encoding="utf-8")
            ok(str(path.relative_to(WORKSPACE_ROOT)))

    replace(WORKSPACE_ROOT / "Cargo.toml",
            r'^(version\s*=\s*)"[^"]+"', rf'\1"{new_version}"')
    # Workspace-deps entry that pins our own crates — needs to follow the
    # workspace package version, otherwise `cargo publish` (and even
    # `cargo update --workspace`) refuses to resolve.
    replace(WORKSPACE_ROOT / "Cargo.toml",
            r'(tomlplus-syntax\s*=\s*\{[^}]*version\s*=\s*)"[^"]+"',
            rf'\1"{new_version}"')
    replace(PY_DIR / "pyproject.toml",
            r'^(version\s*=\s*)"[^"]+"', rf'\1"{new_version}"')
    replace(PY_DIR / "python" / "tomlplus" / "__init__.py",
            r'__version__\s*=\s*"[^"]+"', f'__version__ = "{new_version}"')
    replace(NODE_DIR / "package.json",
            r'("version"\s*:\s*)"[^"]+"', rf'\1"{new_version}"')
    replace(VSCODE_DIR / "package.json",
            r'("version"\s*:\s*)"[^"]+"', rf'\1"{new_version}"')

    cargo_path()
    run(["cargo", "update", "--workspace"], cwd=WORKSPACE_ROOT)

def workspace_version() -> str:
    m = re.search(r'^version\s*=\s*"([^"]+)"',
                  (WORKSPACE_ROOT / "Cargo.toml").read_text(encoding="utf-8"),
                  re.MULTILINE)
    if not m:
        raise CommandError("Could not read workspace version.")
    return m.group(1)

# ── Clean ────────────────────────────────────────────────────────────────────
def action_clean() -> None:
    step("Clean")
    cargo_path()
    run(["cargo", "clean"], cwd=WORKSPACE_ROOT)
    if RELEASE_DIR.exists():
        shutil.rmtree(RELEASE_DIR)
        ok(f"Removed {RELEASE_DIR.relative_to(WORKSPACE_ROOT)}")
    for d in (VSCODE_DIR / "out", VSCODE_DIR / "node_modules"):
        if d.exists():
            shutil.rmtree(d, ignore_errors=True)
    for pattern in ("*.node", "*.vsix", "*.gem", "*.whl", "*.nupkg"):
        for p in WORKSPACE_ROOT.rglob(pattern):
            try: p.unlink()
            except OSError: pass

# ── Build steps ──────────────────────────────────────────────────────────────
def build_rust() -> None:
    step("cargo build --workspace --release")
    cargo_path()
    run(["cargo", "build", "--workspace", "--release"], cwd=WORKSPACE_ROOT)
    ok("Built all Rust crates.")

def build_one_rust(pkg: str) -> None:
    step(f"cargo build -p {pkg} --release")
    cargo_path()
    run(["cargo", "build", "--release", "-p", pkg], cwd=WORKSPACE_ROOT)
    ok(f"Built {pkg}.")

def build_ffi_lib() -> None:
    build_one_rust("tomlplus-ffi")

def build_python() -> None:
    step("Python wheel (maturin develop --release)")
    cargo_path()
    ensure_py_venv()
    run(["maturin", "develop", "--release"], cwd=PY_DIR)
    ok("Installed editable tomlplus wheel into venv.")

def build_node() -> None:
    step("Node native module (napi build)")
    cargo_path()
    if not (NODE_DIR / "node_modules").exists():
        run(["npm", "install", "--silent"], cwd=NODE_DIR)
    run(["npx", "napi", "build", "--platform", "--release"], cwd=NODE_DIR)
    ok("Built .node + index.js/.d.ts.")

def build_vscode() -> None:
    step("VS Code extension (tsc)")
    if not (VSCODE_DIR / "node_modules").exists():
        run(["npm", "install", "--silent"], cwd=VSCODE_DIR)
    run(["npm", "run", "compile"], cwd=VSCODE_DIR)
    ok("Compiled TypeScript → out/extension.js.")

def ensure_wasm_tooling() -> None:
    cargo_path()
    out = run(["rustup", "target", "list", "--installed"], capture=True).stdout
    if "wasm32-unknown-unknown" not in out:
        run(["rustup", "target", "add", "wasm32-unknown-unknown"])
    if not has_tool("wasm-pack"):
        run(["cargo", "install", "wasm-pack", "--quiet"])

def build_wasm() -> None:
    ensure_wasm_tooling()
    step("WASM (wasm-pack for web + nodejs + bundler)")
    # The output dir for `--target nodejs` is named `pkg-node` (not
    # `pkg-nodejs`) so it matches the path used by the WASM crate's own
    # test.mjs + the cross-binding harness.
    out_dirs = {"web": "pkg-web", "nodejs": "pkg-node", "bundler": "pkg-bundler"}
    for t, out in out_dirs.items():
        run(["wasm-pack", "build", "--release", "--target", t, "--out-dir", out], cwd=WASM_DIR)
    ok("Produced pkg-web, pkg-node, pkg-bundler.")

def ensure_cgo_compiler() -> None:
    if has_tool("gcc") or has_tool("clang"):
        return
    candidates = [
        Path(r"C:\Ruby33-x64\msys64\ucrt64\bin"),
        Path(r"C:\Ruby33-x64\msys64\mingw64\bin"),
        Path(r"C:\msys64\ucrt64\bin"),
        Path(r"C:\msys64\mingw64\bin"),
        Path(r"C:\TDM-GCC-64\bin"),
    ]
    for c in candidates:
        if (c / "gcc.exe").exists():
            add_to_path(c)
            info(f"Using cgo compiler: {c}\\gcc.exe")
            return
    raise CommandError("cgo needs a C compiler — install MinGW (e.g. RubyInstaller's DevKit).")

def build_go() -> None:
    build_ffi_lib()
    add_ffi_to_loader_path()
    step("Go (go build)")
    require_tool("go")
    if IS_WINDOWS:
        ensure_cgo_compiler()
    os.environ["CGO_ENABLED"] = "1"
    os.environ["CGO_CFLAGS"]  = f"-I{FFI_DIR / 'include'}"
    os.environ["CGO_LDFLAGS"] = f"-L{TARGET_DIR / 'release'}"
    run(["go", "build", "./..."], cwd=GO_DIR)
    ok("Compiled.")

def ensure_ruby_ffi() -> None:
    """Make the `ffi` gem available to the Ruby on PATH.

    `gem install` in a separate CI step often lands in a different gem
    home than `ruby/setup-ruby`'s active one, so the gem is "installed"
    but `require 'ffi'` still fails. Doing the install ourselves with
    the same `ruby`/`gem` binaries we'll use for the test sidesteps that.
    """
    require_tool("ruby")
    require_tool("gem")
    probe = subprocess.run(
        ["ruby", "-e", "require 'ffi'"],
        capture_output=True, text=True,
    )
    if probe.returncode == 0:
        return
    info("Installing `ffi` gem (not available to the ruby on PATH)")
    run(["gem", "install", "ffi", "--no-document", "--silent"])

def build_ruby() -> None:
    build_ffi_lib()
    add_ffi_to_loader_path()
    step("Ruby (sanity-load gem source)")
    ensure_ruby_ffi()
    # Point Ruby's FFI at the absolute library path. Windows' LoadLibrary
    # ignores PATH; Linux/macOS need the platform-correct lib*.so/dylib name.
    if IS_WINDOWS:
        lib_name = "tomlplus_ffi.dll"
    elif sys.platform == "darwin":
        lib_name = "libtomlplus_ffi.dylib"
    else:
        lib_name = "libtomlplus_ffi.so"
    os.environ["TOMLPLUS_LIB"] = str(TARGET_DIR / "release" / lib_name)
    run(["ruby", "-Ilib", "-e", "require 'tomlplus'; puts Tomlplus::VERSION"], cwd=RUBY_DIR)

GRADLE_VERSION = "8.10.2"

def gradle_bin() -> Path:
    """Returns the path to a Gradle executable, bootstrapping a local copy if
    the system doesn't have one. The distribution is cached under
    `bindings/tomlplus-java/.gradle-local/` so subsequent calls are instant."""
    cache    = JAVA_DIR / ".gradle-local"
    dist_dir = cache / f"gradle-{GRADLE_VERSION}" / "bin"
    gradle   = dist_dir / ("gradle.bat" if IS_WINDOWS else "gradle")
    if gradle.exists():
        return gradle

    sys_gradle = has_tool("gradle")
    if sys_gradle:
        return sys_gradle

    cache.mkdir(parents=True, exist_ok=True)
    zip_path = cache / f"gradle-{GRADLE_VERSION}-bin.zip"
    url      = f"https://services.gradle.org/distributions/gradle-{GRADLE_VERSION}-bin.zip"
    if not zip_path.exists():
        info(f"Downloading Gradle {GRADLE_VERSION} from {url}")
        urllib.request.urlretrieve(url, str(zip_path))
    info(f"Extracting Gradle {GRADLE_VERSION} …")
    with zipfile.ZipFile(zip_path) as z:
        z.extractall(cache)
    if not gradle.exists():
        raise CommandError(f"Gradle {GRADLE_VERSION} did not unpack to {gradle}.")
    if not IS_WINDOWS:
        gradle.chmod(gradle.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return gradle

def build_java() -> None:
    build_ffi_lib()
    add_ffi_to_loader_path()
    step("Java (gradle assemble)")
    g = gradle_bin()
    os.environ["TOMLPLUS_LIB_DIR"] = str(TARGET_DIR / "release")
    run([str(g), "-q", "--no-daemon", "assemble"], cwd=JAVA_DIR)
    ok("Compiled.")

def build_dotnet() -> None:
    build_ffi_lib()
    add_ffi_to_loader_path()
    step("Dotnet (dotnet build)")
    require_tool("dotnet")
    run(["dotnet", "build", "--nologo"], cwd=DOTNET_DIR)

# ── Test steps ───────────────────────────────────────────────────────────────
def test_rust() -> None:
    step("cargo test --workspace")
    cargo_path()
    run(["cargo", "test", "--workspace"], cwd=WORKSPACE_ROOT)
    ok("All Rust tests pass.")

def test_python() -> None:
    step("Python compat suite (pytest)")
    py = ensure_py_venv()
    run([str(py), "-m", "pytest", "tests", "-q"], cwd=PY_DIR)
    ok("Python wheel matches existing tomlplus API.")

def test_node() -> None:
    step("Node tests")
    run(["node", "--test", "test.mjs"], cwd=NODE_DIR)
    ok("Node native module tests pass.")

def test_wasm() -> None:
    build_wasm()
    step("WASM tests (node --test against pkg-node)")
    if not (WASM_DIR / "pkg-node" / "tomlplus_wasm.js").exists():
        run(["wasm-pack", "build", "--release", "--target", "nodejs", "--out-dir", "pkg-node"], cwd=WASM_DIR)
    run(["node", "--test", "test.mjs"], cwd=WASM_DIR)

def test_go() -> None:
    build_go()
    step("go test ./...")
    run(["go", "test", "./..."], cwd=GO_DIR)

def test_ruby() -> None:
    build_ruby()
    step("Ruby tests (minitest)")
    run(["ruby", "-Ilib", "-Itest", "test/test_tomlplus.rb"], cwd=RUBY_DIR)

def test_java() -> None:
    build_ffi_lib()
    add_ffi_to_loader_path()
    step("Java tests (gradle test)")
    g = gradle_bin()
    os.environ["TOMLPLUS_LIB_DIR"] = str(TARGET_DIR / "release")
    run([str(g), "-q", "--no-daemon", "test"], cwd=JAVA_DIR)
    ok("JUnit 5 passed.")

def test_dotnet() -> None:
    add_ffi_to_loader_path()
    step("Dotnet tests")
    require_tool("dotnet")
    run(["dotnet", "test", "--nologo"], cwd=DOTNET_DIR / "Tomlplus.Tests")

def test_cross_binding() -> None:
    """Parse one canonical fixture via every binding and verify identical JSON."""
    # Build everything every binding needs: FFI, CLI (for the canonical baseline),
    # wheel, WASM module. Skip the actual Rust test suite — that's tested
    # elsewhere — but make sure native artefacts exist.
    build_ffi_lib()
    add_ffi_to_loader_path()
    build_one_rust("tomlplus-cli")
    build_python()
    build_node()
    build_wasm()
    # cgo needs a C compiler in $PATH for the Go harness to build.
    if IS_WINDOWS:
        try: ensure_cgo_compiler()
        except CommandError: pass  # orchestrator will mark as skipped
    step("Cross-binding integration test")
    py = ensure_py_venv()
    runner = WORKSPACE_ROOT / "tests" / "cross-binding" / "run.py"
    run([str(py), str(runner)])

# ── Package steps ────────────────────────────────────────────────────────────
def new_release_dir() -> None:
    RELEASE_DIR.mkdir(parents=True, exist_ok=True)

def stage(p: Path) -> None:
    new_release_dir()
    dest = RELEASE_DIR / p.name
    shutil.copy2(p, dest)
    ok(f"Staged {p.name}")

def package_python() -> None:
    step("Package: Python wheel")
    cargo_path()
    ensure_py_venv()
    run(["maturin", "build", "--release"], cwd=PY_DIR)
    for whl in (TARGET_DIR / "wheels").glob("*.whl"):
        stage(whl)

def package_node() -> None:
    step("Package: Node .node + npm package")
    build_node()
    for n in NODE_DIR.glob("*.node"):
        stage(n)

def package_wasm() -> None:
    build_wasm()
    step("Package: WASM npm bundles")
    new_release_dir()
    for sub in ("pkg-web", "pkg-nodejs", "pkg-bundler", "pkg-node"):
        d = WASM_DIR / sub
        if not d.exists(): continue
        zip_path = RELEASE_DIR / f"tomlplus-wasm-{sub}.zip"
        with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
            for f in d.rglob("*"):
                if f.is_file():
                    z.write(f, f.relative_to(d))
        ok(f"Staged {zip_path.name}")

def package_ffi() -> None:
    step("Package: C library tarball")
    build_one_rust("tomlplus-ffi")
    new_release_dir()
    ver = workspace_version()
    arch = "windows-x86_64" if IS_WINDOWS else f"{sys.platform}-x86_64"
    bundle = RELEASE_DIR / f"tomlplus-ffi-{ver}-{arch}"
    if bundle.exists(): shutil.rmtree(bundle)
    bundle.mkdir()
    for f in ("tomlplus_ffi.dll", "tomlplus_ffi.dll.lib", "tomlplus_ffi.lib",
              "libtomlplus_ffi.so", "libtomlplus_ffi.dylib", "libtomlplus_ffi.a"):
        src = TARGET_DIR / "release" / f
        if src.exists(): shutil.copy2(src, bundle / f)
    shutil.copy2(FFI_DIR / "include" / "tomlplus.h", bundle)
    zip_path = bundle.with_suffix(".zip")
    if zip_path.exists(): zip_path.unlink()
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
        for f in bundle.rglob("*"):
            if f.is_file(): z.write(f, f.relative_to(bundle.parent))
    ok(f"Staged {zip_path.name}")

def package_lsp_binary() -> None:
    step("Package: tomlplus-lsp binary")
    build_one_rust("tomlplus-lsp")
    stage(TARGET_DIR / "release" / f"tomlplus-lsp{EXE}")

def package_cli_binary() -> None:
    step("Package: tomlpr CLI")
    build_one_rust("tomlplus-cli")
    stage(TARGET_DIR / "release" / f"tomlpr{EXE}")

def package_vscode() -> None:
    step("Package: VS Code .vsix")
    build_vscode()
    new_release_dir()
    run(["npx", "vsce", "package", "--out", str(RELEASE_DIR)], cwd=VSCODE_DIR)
    ok(f"Staged .vsix → {RELEASE_DIR.name}/")

def package_java() -> None:
    build_java()
    step("Package: Java jars + Maven-staged layout")
    g = gradle_bin()
    # Vanniktech's plugin gives us `publishToMavenLocal` (per-publication)
    # which writes a full Maven layout (jar + sources + javadoc + .module
    # + .pom + signatures if a key is configured) into the project's local
    # Maven repo. Use `~/.m2`-shaped output we can ship if needed.
    run([str(g), "-q", "--no-daemon", "publishToMavenLocal"], cwd=JAVA_DIR)
    new_release_dir()
    for j in (JAVA_DIR / "build" / "libs").glob("*.jar"):
        stage(j)

def package_dotnet() -> None:
    build_dotnet()
    step("Package: NuGet .nupkg")
    new_release_dir()
    run(["dotnet", "pack", "--nologo", "--configuration", "Release",
         "--output", str(RELEASE_DIR)], cwd=DOTNET_DIR)
    ok("Staged .nupkg.")

def package_ruby() -> None:
    build_ruby()
    step("Package: Ruby gem")
    run(["gem", "build", "tomlplus.gemspec"], cwd=RUBY_DIR)
    new_release_dir()
    for g in RUBY_DIR.glob("*.gem"):
        shutil.move(str(g), RELEASE_DIR / g.name)
        ok(f"Staged {g.name}")

def package_go() -> None:
    build_go()
    step("Package: Go module")
    info("Go modules publish by git tag — no artefact to stage.")

# ── Publish steps ────────────────────────────────────────────────────────────
def publish_python(dry_run: bool) -> None:
    package_python()
    if dry_run:
        skip("DryRun — skipping twine upload")
        return
    token = require_env("PYPI_TOKEN", "upload Python wheels to PyPI")
    ensure_py_venv()
    wheels = list(RELEASE_DIR.glob("tomlplus-*.whl"))
    if not wheels:
        raise CommandError("No wheels staged.")
    env = os.environ.copy()
    env["TWINE_USERNAME"] = "__token__"
    env["TWINE_PASSWORD"] = token
    run(["twine", "upload", "--non-interactive", *[str(w) for w in wheels]], env=env)
    ok("Published to PyPI.")

def publish_node(dry_run: bool) -> None:
    package_node()
    if dry_run:
        run(["npm", "publish", "--access", "public", "--dry-run"], cwd=NODE_DIR)
        return
    run(["npx", "napi", "prepublish", "--skip-gh-release"], cwd=NODE_DIR)
    run(["npm", "publish", "--access", "public"], cwd=NODE_DIR)
    ok("Published to npm.")

def publish_wasm(dry_run: bool) -> None:
    package_wasm()
    pkg = WASM_DIR / "pkg-bundler"
    if not (pkg / "package.json").exists():
        run(["wasm-pack", "build", "--release", "--target", "bundler", "--out-dir", "pkg-bundler"], cwd=WASM_DIR)
    if dry_run:
        run(["npm", "publish", "--access", "public", "--dry-run"], cwd=pkg)
        return
    run(["npm", "publish", "--access", "public"], cwd=pkg)
    ok("Published tomlplus-wasm to npm.")

def publish_crates(dry_run: bool) -> None:
    step("Publish: crates.io (in dependency order)")
    cargo_path()
    plan = [("tomlplus-syntax", True), ("tomlplus-ffi", False),
            ("tomlplus-cli", False), ("tomlplus-lsp", False)]
    if dry_run:
        run(["cargo", "publish", "-p", "tomlplus-syntax", "--dry-run"], cwd=WORKSPACE_ROOT)
        for name, _ in plan[1:]:
            skip(f"DryRun: {name} — would publish after parent indexes on crates.io.")
        return
    require_env("CARGO_REGISTRY_TOKEN", "publish Rust crates")
    for name, verify in plan:
        args = ["cargo", "publish", "-p", name]
        if not verify:
            args.append("--no-verify")
        run(args, cwd=WORKSPACE_ROOT)
        if verify:
            time.sleep(10)  # let crates.io index
    ok("All Rust crates published.")

def publish_vscode(dry_run: bool) -> None:
    """Publish the VS Code extension. Both stores are optional — set
    `VSCE_PAT` for the Microsoft Marketplace, `OVSX_PAT` for Open VSX,
    or both. With neither set, the .vsix is still produced under
    `release/` so users can install it manually from a GitHub Release.
    """
    package_vscode()
    if dry_run:
        skip("DryRun — skipping marketplace + Open VSX pushes")
        return

    vsce_pat = env_get("VSCE_PAT")
    ovsx_pat = env_get("OVSX_PAT")

    if not vsce_pat and not ovsx_pat:
        warn("Neither VSCE_PAT nor OVSX_PAT set — .vsix staged for manual install only.")
        info("Users will install via `code --install-extension <release/*.vsix>`.")
        return

    if vsce_pat:
        env = os.environ.copy(); env["VSCE_PAT"] = vsce_pat
        run(["npx", "vsce", "publish", "--no-dependencies"], cwd=VSCODE_DIR, env=env)
        ok("Published to VS Code Marketplace.")
    else:
        skip("VSCE_PAT not set — skipped VS Code Marketplace.")

    if ovsx_pat:
        vsix = next(iter(RELEASE_DIR.glob("tomlplus-*.vsix")), None)
        if vsix:
            run(["npx", "ovsx", "publish", str(vsix), "-p", ovsx_pat])
            ok("Published to Open VSX.")
        else:
            warn("No .vsix found in release/ — skipping Open VSX upload.")
    else:
        skip("OVSX_PAT not set — skipped Open VSX.")

def publish_java(dry_run: bool) -> None:
    """Publish to Sonatype Central Portal via the Vanniktech plugin.

    Credentials come from environment variables that the Vanniktech plugin
    auto-reads (it accepts `ORG_GRADLE_PROJECT_*` env vars as Gradle project
    properties):

        CENTRAL_USERNAME  → ORG_GRADLE_PROJECT_mavenCentralUsername
        CENTRAL_PASSWORD  → ORG_GRADLE_PROJECT_mavenCentralPassword
        SIGN_KEY          → ORG_GRADLE_PROJECT_signingInMemoryKey
        SIGN_PASSWORD     → ORG_GRADLE_PROJECT_signingInMemoryKeyPassword

    We translate the friendlier names into the Gradle-property-style names
    here so the GitHub-Actions secret names stay readable.
    """
    package_java()
    g = gradle_bin()

    if dry_run:
        skip("DryRun — staging to ~/.m2 only (no Central upload).")
        run([str(g), "-q", "--no-daemon", "publishToMavenLocal"], cwd=JAVA_DIR)
        return

    # No-creds path: stage but don't upload. Useful when MAVEN secrets
    # aren't configured yet but you still want a full `release.py` to run.
    if not (env_get("CENTRAL_USERNAME") and env_get("CENTRAL_PASSWORD")):
        warn("CENTRAL_USERNAME/CENTRAL_PASSWORD not set; staging to ~/.m2 only.")
        run([str(g), "-q", "--no-daemon", "publishToMavenLocal"], cwd=JAVA_DIR)
        return

    # Vanniktech reads ORG_GRADLE_PROJECT_*; map our friendlier names.
    env = os.environ.copy()
    env["ORG_GRADLE_PROJECT_mavenCentralUsername"] = env_get("CENTRAL_USERNAME") or ""
    env["ORG_GRADLE_PROJECT_mavenCentralPassword"] = env_get("CENTRAL_PASSWORD") or ""
    if env_get("SIGN_KEY"):
        env["ORG_GRADLE_PROJECT_signingInMemoryKey"] = env_get("SIGN_KEY") or ""
    if env_get("SIGN_PASSWORD"):
        env["ORG_GRADLE_PROJECT_signingInMemoryKeyPassword"] = env_get("SIGN_PASSWORD") or ""

    step("Publish → Sonatype Central Portal (publishAndReleaseToMavenCentral)")
    # `publishAndReleaseToMavenCentral` uploads to Central and then releases
    # the staging bundle. If you'd rather inspect the staged bundle in the
    # Central UI before promotion, swap to `publishToMavenCentral`.
    run([str(g), "-q", "--no-daemon", "publishAndReleaseToMavenCentral"], cwd=JAVA_DIR, env=env)
    ok("Deployed to Maven Central (io.github.carsonkopec:tomlplus-java).")

def publish_dotnet(dry_run: bool) -> None:
    package_dotnet()
    feed = env_get("NUGET_FEED_URL") or "https://api.nuget.org/v3/index.json"
    nupkg = next(iter(RELEASE_DIR.glob("Tomlplus.*.nupkg")), None)
    if not nupkg:
        raise CommandError("No .nupkg found in release/.")
    if dry_run:
        skip(f"DryRun — would push {nupkg.name} to {feed}")
        return
    api_key = require_env("NUGET_API_KEY", "publish a NuGet package")
    step(f"Publish → {feed} ({nupkg.name})")
    run(["dotnet", "nuget", "push", str(nupkg),
         "--api-key", api_key, "--source", feed, "--skip-duplicate"])
    ok(f"Pushed to {feed}.")

def publish_ruby(dry_run: bool) -> None:
    package_ruby()
    if dry_run:
        skip("DryRun — skipping gem push")
        return
    gem = next(iter(RELEASE_DIR.glob("tomlplus-*.gem")), None)
    if not gem:
        raise CommandError("No .gem found in release/.")
    step(f"Publish → RubyGems ({gem.name})")
    run(["gem", "push", str(gem)])
    ok("Pushed to RubyGems.")

def publish_go(dry_run: bool) -> None:
    step("Publish: Go module")
    if dry_run:
        skip("DryRun — Go modules publish by git tag; no command would run here.")
        return
    info(f"Go modules use the workspace git tag (e.g. v{workspace_version()}).")
    info(f"After tag push, `go get github.com/<owner>/tomlplus/bindings/tomlplus-go@v{workspace_version()}` works.")
    ok("No additional registry push needed.")

def publish_github_release(dry_run: bool) -> None:
    step("Publish: GitHub Release with binaries")
    if dry_run:
        skip("DryRun — skipping gh release create")
        _list_release_dir()
        return
    require_env("GITHUB_TOKEN", "create a GitHub Release")
    require_tool("gh", "Install via winget install GitHub.cli")
    tag = f"v{workspace_version()}"
    assets = [str(p) for p in RELEASE_DIR.iterdir() if p.is_file()]
    if not assets:
        raise CommandError(f"No staged assets in {RELEASE_DIR}.")
    run(["gh", "release", "create", tag, "--title", f"TOML+ {tag}",
         "--generate-notes", *assets])
    ok(f"Released {tag} with {len(assets)} assets.")

def _list_release_dir() -> None:
    if not RELEASE_DIR.exists():
        return
    table = Table(title=f"{RELEASE_DIR.name}/", show_header=True, header_style="bold cyan")
    table.add_column("Name")
    table.add_column("Size", justify="right")
    for p in sorted(RELEASE_DIR.iterdir()):
        size = p.stat().st_size if p.is_file() else 0
        table.add_row(p.name, f"{size:,}")
    console.print(table)

# ── Dispatchers ──────────────────────────────────────────────────────────────
TARGETS_RUST     = ("syntax", "ffi", "cli", "lsp")
TARGETS_BINDINGS = ("python", "node", "wasm", "go", "ruby", "java", "dotnet")

def do_build(target: str) -> None:
    {
        "syntax":  lambda: build_one_rust("tomlplus-syntax"),
        "ffi":     build_ffi_lib,
        "cli":     lambda: build_one_rust("tomlplus-cli"),
        "lsp":     lambda: build_one_rust("tomlplus-lsp"),
        "rust":    build_rust,
        "python":  build_python,
        "node":    build_node,
        "wasm":    build_wasm,
        "vscode":  build_vscode,
        "go":      build_go,
        "ruby":    build_ruby,
        "java":    build_java,
        "dotnet":  build_dotnet,
        "c-bindings": lambda: (build_ffi_lib(), build_go(), build_ruby(), build_java(), build_dotnet()),
        # cross-binding is a *test*, not a build; do_build for it is a no-op.
        "cross-binding": lambda: None,
        "all":     lambda: (build_rust(), build_python(), build_node(), build_vscode(),
                            build_wasm(), build_go(), build_ruby(), build_java(), build_dotnet()),
    }[target]()

def do_test(target: str) -> None:
    do_build(target)
    {
        "syntax": test_rust, "ffi": test_rust, "cli": test_rust, "lsp": test_rust, "rust": test_rust,
        "python": test_python, "node": test_node, "wasm": test_wasm,
        "go": test_go, "ruby": test_ruby, "java": test_java, "dotnet": test_dotnet,
        "vscode": lambda: skip("No automated tests for the VS Code extension."),
        "c-bindings": lambda: (test_rust(), test_go(), test_ruby(), test_java(), test_dotnet()),
        "cross-binding": test_cross_binding,
        "all":     lambda: (test_rust(), test_python(), test_node(), test_wasm(),
                            test_java(), test_dotnet(), test_cross_binding()),
    }[target]()

def do_package(target: str) -> None:
    do_test(target)
    {
        "ffi": package_ffi, "cli": package_cli_binary, "lsp": package_lsp_binary,
        "python": package_python, "node": package_node, "wasm": package_wasm,
        "vscode": package_vscode,
        "go": package_go, "ruby": package_ruby, "java": package_java, "dotnet": package_dotnet,
        "rust": lambda: (package_ffi(), package_cli_binary(), package_lsp_binary()),
        "c-bindings": lambda: (package_ffi(), package_go(), package_ruby(), package_java(), package_dotnet()),
        "all": lambda: (package_ffi(), package_cli_binary(), package_lsp_binary(),
                        package_python(), package_node(), package_wasm(), package_vscode(),
                        package_java(), package_dotnet(), package_ruby()),
        "syntax": lambda: None,
    }[target]()
    _list_release_dir()

def do_publish(target: str, dry_run: bool) -> None:
    do_package(target)
    pub_map: dict[str, Callable[[bool], None]] = {
        "python":  publish_python,
        "node":    publish_node,
        "wasm":    publish_wasm,
        "rust":    publish_crates,
        "vscode":  publish_vscode,
        "go":      publish_go,
        "ruby":    publish_ruby,
        "java":    publish_java,
        "dotnet":  publish_dotnet,
    }
    if target == "c-bindings":
        for fn in (publish_go, publish_ruby, publish_java, publish_dotnet):
            fn(dry_run)
    elif target == "all":
        publish_crates(dry_run)
        publish_python(dry_run)
        publish_node(dry_run)
        publish_wasm(dry_run)
        publish_vscode(dry_run)
        publish_java(dry_run)
        publish_dotnet(dry_run)
        publish_ruby(dry_run)
        publish_go(dry_run)
        publish_github_release(dry_run)
    elif target in pub_map:
        pub_map[target](dry_run)
    else:
        skip(f"No publish step for target '{target}'.")

def do_release(new_version: str, dry_run: bool) -> None:
    update_version(new_version)
    step("git status")
    run(["git", "status", "--short"], cwd=WORKSPACE_ROOT, check=False)
    if not dry_run:
        step(f"git commit + tag v{new_version}")
        run(["git", "add", "-A"], cwd=WORKSPACE_ROOT)
        run(["git", "commit", "-m", f"release: v{new_version}"], cwd=WORKSPACE_ROOT)
        run(["git", "tag", f"v{new_version}"], cwd=WORKSPACE_ROOT)
    else:
        skip("DryRun — skipping git commit + tag.")
    do_publish("all", dry_run)
    if not dry_run:
        step("git push + push tag")
        run(["git", "push"], cwd=WORKSPACE_ROOT)
        run(["git", "push", "--tags"], cwd=WORKSPACE_ROOT)

# ── CLI ──────────────────────────────────────────────────────────────────────
ALL_TARGETS = ("all", "syntax", "ffi", "cli", "lsp", "rust",
               "python", "node", "wasm", "vscode",
               "go", "ruby", "java", "dotnet", "c-bindings",
               "cross-binding")

def print_banner(action: str, target: str, dry_run: bool) -> None:
    body = Text()
    body.append("action ", style="dim"); body.append(action, style="step")
    body.append("    target ", style="dim"); body.append(target, style="target")
    body.append("    dry-run ", style="dim")
    body.append(str(bool(dry_run)).lower(), style="warn" if dry_run else "skip")
    console.print(Panel(body, title="tomlplus-release", border_style="cyan", padding=(0, 2)))

def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="release.py", description="TOML+ release automation")
    sub = p.add_subparsers(dest="action", required=True)

    def add_target(sp: argparse.ArgumentParser) -> None:
        sp.add_argument("-t", "--target", choices=ALL_TARGETS, default="all")

    for name in ("build", "test", "package"):
        add_target(sub.add_parser(name))

    sp = sub.add_parser("publish")
    add_target(sp)
    sp.add_argument("--dry-run", action="store_true")

    sp = sub.add_parser("version")
    sp.add_argument("--new", required=True, help="X.Y.Z")

    sub.add_parser("clean")

    sp = sub.add_parser("release")
    sp.add_argument("--new", required=True, help="X.Y.Z")
    sp.add_argument("--dry-run", action="store_true")

    return p

def main() -> int:
    args = build_parser().parse_args()
    dry_run = getattr(args, "dry_run", False)
    target  = getattr(args, "target", "all")
    print_banner(args.action, target if args.action not in ("version", "clean", "release") else "—", dry_run)

    started = time.time()
    try:
        if   args.action == "build":   do_build(target)
        elif args.action == "test":    do_test(target)
        elif args.action == "package": do_package(target)
        elif args.action == "publish": do_publish(target, dry_run)
        elif args.action == "version": update_version(args.new)
        elif args.action == "clean":   action_clean()
        elif args.action == "release": do_release(args.new, dry_run)
        else: raise CommandError(f"unknown action: {args.action}")
    except CommandError as e:
        console.print()
        console.print(Panel(Text(str(e), style="err"), title="failed", border_style="red"))
        return 1
    except subprocess.CalledProcessError as e:
        console.print()
        console.print(Panel(Text(str(e), style="err"), title="subprocess failure", border_style="red"))
        return e.returncode or 1
    except KeyboardInterrupt:
        console.print("\n[warn]interrupted[/warn]")
        return 130

    elapsed = time.time() - started
    console.print()
    console.print(f"[dim]Finished in {elapsed:,.1f}s.[/dim]")
    return 0

if __name__ == "__main__":
    sys.exit(main())
