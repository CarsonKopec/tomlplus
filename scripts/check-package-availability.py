#!/usr/bin/env python3
"""Check which TOML+ package names are still claimable on each registry.

Hits the public API of every registry we publish to (crates.io, PyPI, npm,
RubyGems, NuGet, Open VSX, Maven Central, VS Code Marketplace) for the exact
names declared in our manifests, and reports:

  AVAILABLE — name is unclaimed, you can publish at this name
  TAKEN     — somebody already owns it; you'll need a different name OR
              proof that you own it (which this script can't determine)
  UNKNOWN   — the registry didn't give a clean yes/no (network error,
              non-standard response, rate limit)

This is a *read-only* check — no auth, no side effects. Safe to run anytime.

Usage:
    py -3 scripts/check-package-availability.py
"""

from __future__ import annotations

import json
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Callable

# Auto-install rich on first run (mirrors release.py).
try:
    from rich.console import Console
    from rich.table import Table
    from rich.theme import Theme
except ImportError:
    import subprocess
    subprocess.run([sys.executable, "-m", "pip", "install", "--quiet", "rich"], check=True)
    from rich.console import Console
    from rich.table import Table
    from rich.theme import Theme

# Force UTF-8 console on Windows so rich's glyphs don't crash cp1252.
for _s in (sys.stdout, sys.stderr):
    try:
        _s.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, ValueError):
        pass

console = Console(theme=Theme({
    "available": "bold green",
    "taken":     "bold red",
    "unknown":   "bold yellow",
    "registry":  "cyan",
    "pkg":       "bold",
    "url":       "dim",
}), highlight=False)


# ── HTTP helper ──────────────────────────────────────────────────────────────
USER_AGENT = "tomlplus-check-availability/1.0 (+https://github.com/CarsonKopec/tomlplus)"

def http_get(url: str, *, timeout: float = 10.0) -> tuple[int, str]:
    """Return (status, body). 404 surfaces as (404, '') rather than raising."""
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": "*/*"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.read().decode("utf-8", errors="replace")
    except urllib.error.HTTPError as e:
        return e.code, ""
    except (urllib.error.URLError, TimeoutError) as e:
        raise RuntimeError(f"network error: {e}") from e


# ── Per-registry probes ──────────────────────────────────────────────────────
@dataclass
class Result:
    name: str
    registry: str
    status: str           # "available" | "taken" | "unknown"
    url: str
    detail: str = ""

def check_crates_io(name: str) -> Result:
    url = f"https://crates.io/api/v1/crates/{name}"
    try:
        code, body = http_get(url)
    except RuntimeError as e:
        return Result(name, "crates.io", "unknown", url, str(e))
    if code == 404: return Result(name, "crates.io", "available", url)
    if code == 200: return Result(name, "crates.io", "taken", url,
        json.loads(body).get("crate", {}).get("max_version", ""))
    return Result(name, "crates.io", "unknown", url, f"HTTP {code}")

def check_pypi(name: str) -> Result:
    url = f"https://pypi.org/pypi/{name}/json"
    try:
        code, body = http_get(url)
    except RuntimeError as e:
        return Result(name, "PyPI", "unknown", url, str(e))
    if code == 404: return Result(name, "PyPI", "available", url)
    if code == 200:
        info = json.loads(body).get("info", {})
        return Result(name, "PyPI", "taken", url, info.get("version", ""))
    return Result(name, "PyPI", "unknown", url, f"HTTP {code}")

def check_npm(name: str) -> Result:
    url = f"https://registry.npmjs.org/{name}"
    try:
        code, body = http_get(url)
    except RuntimeError as e:
        return Result(name, "npm", "unknown", url, str(e))
    if code == 404: return Result(name, "npm", "available", url)
    if code == 200:
        latest = json.loads(body).get("dist-tags", {}).get("latest", "")
        return Result(name, "npm", "taken", url, latest)
    return Result(name, "npm", "unknown", url, f"HTTP {code}")

def check_rubygems(name: str) -> Result:
    url = f"https://rubygems.org/api/v1/gems/{name}.json"
    try:
        code, body = http_get(url)
    except RuntimeError as e:
        return Result(name, "RubyGems", "unknown", url, str(e))
    if code == 404: return Result(name, "RubyGems", "available", url)
    if code == 200:
        return Result(name, "RubyGems", "taken", url,
                      json.loads(body).get("version", ""))
    return Result(name, "RubyGems", "unknown", url, f"HTTP {code}")

def check_nuget(name: str) -> Result:
    # NuGet flat-container is case-insensitive — they normalise to lowercase.
    url = f"https://api.nuget.org/v3-flatcontainer/{name.lower()}/index.json"
    try:
        code, body = http_get(url)
    except RuntimeError as e:
        return Result(name, "NuGet", "unknown", url, str(e))
    if code == 404: return Result(name, "NuGet", "available", url)
    if code == 200:
        versions = json.loads(body).get("versions", [])
        latest = versions[-1] if versions else ""
        return Result(name, "NuGet", "taken", url, latest)
    return Result(name, "NuGet", "unknown", url, f"HTTP {code}")

def check_open_vsx(publisher: str, name: str) -> Result:
    url = f"https://open-vsx.org/api/{publisher}/{name}"
    try:
        code, body = http_get(url)
    except RuntimeError as e:
        return Result(f"{publisher}.{name}", "Open VSX", "unknown", url, str(e))
    if code == 404: return Result(f"{publisher}.{name}", "Open VSX", "available", url)
    if code == 200:
        ver = json.loads(body).get("version", "")
        return Result(f"{publisher}.{name}", "Open VSX", "taken", url, ver)
    return Result(f"{publisher}.{name}", "Open VSX", "unknown", url, f"HTTP {code}")

def check_marketplace(publisher: str, name: str) -> Result:
    # The Marketplace doesn't have a clean public API; the gallery query is
    # POST-only and brittle. We do a HEAD on the public listing URL and treat
    # 200 = taken, 404 = available.
    listing = f"https://marketplace.visualstudio.com/items?itemName={publisher}.{name}"
    try:
        code, _ = http_get(listing)
    except RuntimeError as e:
        return Result(f"{publisher}.{name}", "VS Code MP", "unknown", listing, str(e))
    if code == 404: return Result(f"{publisher}.{name}", "VS Code MP", "available", listing)
    if code == 200: return Result(f"{publisher}.{name}", "VS Code MP", "taken", listing)
    return Result(f"{publisher}.{name}", "VS Code MP", "unknown", listing, f"HTTP {code}")

def check_maven_central_namespace(group_id: str) -> Result:
    # Central Portal doesn't expose "is this namespace claimed" without auth.
    # Best we can do is search for any artefact whose groupId matches.
    url = f"https://search.maven.org/solrsearch/select?q=g:%22{group_id}%22&rows=1&wt=json"
    try:
        code, body = http_get(url)
    except RuntimeError as e:
        return Result(group_id, "Maven Central", "unknown", url, str(e))
    if code != 200:
        return Result(group_id, "Maven Central", "unknown", url, f"HTTP {code}")
    docs = json.loads(body).get("response", {}).get("docs", [])
    if docs:
        first = docs[0].get("a", "")
        return Result(group_id, "Maven Central", "taken", url, f"existing artefact: {first}")
    return Result(group_id, "Maven Central", "available", url, "(name claim is via Sonatype Central Portal UI)")


# ── Probe plan ───────────────────────────────────────────────────────────────
PROBES: list[Callable[[], Result]] = [
    # Rust crates (everything we publish from the workspace)
    lambda: check_crates_io("tomlplus-syntax"),
    lambda: check_crates_io("tomlplus-ffi"),
    lambda: check_crates_io("tomlplus-cli"),
    lambda: check_crates_io("tomlplus-lsp"),
    lambda: check_crates_io("tomlpr"),

    # Python
    lambda: check_pypi("tomlplus"),

    # Node
    lambda: check_npm("tomlplus"),
    lambda: check_npm("tomlplus-wasm"),

    # Ruby
    lambda: check_rubygems("tomlplus"),

    # NuGet
    lambda: check_nuget("Tomlplus"),

    # Maven Central
    lambda: check_maven_central_namespace("io.github.carsonkopec"),

    # VS Code Marketplace + Open VSX
    lambda: check_marketplace("CarsonKopec", "tomlplus"),
    lambda: check_open_vsx("CarsonKopec", "tomlplus"),
]


# ── Run + report ─────────────────────────────────────────────────────────────
def main() -> int:
    console.print()
    console.print("[bold]TOML+ package-name availability check[/bold]")
    console.print("[dim]Read-only. No auth, no side effects.[/dim]")
    console.print()

    results: list[Result] = []
    with console.status("[dim]Querying registries…[/dim]"):
        for probe in PROBES:
            try:
                results.append(probe())
            except Exception as e:
                results.append(Result("<probe-failed>", "?", "unknown", "", str(e)))

    table = Table(show_header=True, header_style="bold cyan")
    table.add_column("Registry", style="registry")
    table.add_column("Package")
    table.add_column("Status")
    table.add_column("Detail / URL", style="url", overflow="fold")
    for r in results:
        if   r.status == "available": tag = "[available]✓ AVAILABLE[/available]"
        elif r.status == "taken":     tag = "[taken]✗ TAKEN[/taken]"
        else:                          tag = "[unknown]? UNKNOWN[/unknown]"
        detail = r.detail or r.url
        table.add_row(r.registry, f"[pkg]{r.name}[/pkg]", tag, detail)
    console.print(table)

    # Summary
    avail = sum(1 for r in results if r.status == "available")
    taken = sum(1 for r in results if r.status == "taken")
    unk   = sum(1 for r in results if r.status == "unknown")
    console.print()
    console.print(
        f"[available]{avail} available[/available]  ·  "
        f"[taken]{taken} taken[/taken]  ·  "
        f"[unknown]{unk} unknown[/unknown]"
    )
    if taken:
        console.print()
        console.print("[dim]NOTE: a TAKEN result doesn't mean you can't publish — it may be[/dim]")
        console.print("[dim]      yours already. Log in to the registry to confirm ownership.[/dim]")

    return 0 if unk == 0 else 1

if __name__ == "__main__":
    sys.exit(main())
