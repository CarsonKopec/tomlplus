# TOML+ for VS Code

Language support for [TOML+](https://github.com/CarsonKopec/tomlplus), a
config language: TOML with block dictionaries, annotations, and variables.

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

## Features

* **Syntax highlighting** — sections, block dicts, annotations,
  variables, every numeric literal form.
* **Live diagnostics** — parse errors *and* annotation-driven validation
  (`@required`, `@type`, `@min/max`, `@enum`, `@pattern`, `@deprecated`,
  etc.) update as you type.
* **Hover** — show a key's resolved value + type + every annotation +
  any `@tag` metadata. On a `$variable`, show its `[vars]` definition or
  the resolved env-var value.
* **Completion** — snippet-driven for `@annotation`s and `$variable`s,
  with per-item documentation.
* **Inlay hints** — `→ "resolved"` next to `$ENV.X` and `$user_var`
  references, `: int` next to `@type:`-annotated keys.
* **Goto definition** — `Ctrl+Click` a `$var` jumps to its `[vars]` entry.
* **Document symbols** — outline view with nested sections.
* **Format** — round-trip the file through the dumper.
* **Color provider** — `#FF8800` strings get a clickable swatch.
* **Semantic tokens** — 11 token types so your theme can colour
  annotations, variables, sections distinctly.

## Requirements

Needs the `tomlplus-lsp` binary on your PATH (or set
`tomlplus.serverPath` in your VS Code settings).

```pwsh
cargo install tomlplus-lsp
# or download from https://github.com/CarsonKopec/tomlplus/releases
```

## Extension settings

| Setting | Default | Purpose |
| --- | --- | --- |
| `tomlplus.serverPath` | `tomlplus-lsp` | Path to the language server binary. Set to an absolute path if it isn't on PATH. |

## Release notes

See [CHANGELOG.md](https://github.com/CarsonKopec/tomlplus/blob/master/CHANGELOG.md).

## License

[MIT](LICENSE)
