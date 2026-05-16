# tomlplus (Ruby gem)

Ruby bindings to `tomlplus_ffi` via the [`ffi`](https://github.com/ffi/ffi) gem.

## Install

```pwsh
gem install tomlplus
```

The gem loads `tomlplus_ffi.dll` / `libtomlplus_ffi.so` / `libtomlplus_ffi.dylib`
at runtime from the loader search path. Build it first with
`cargo build --release -p tomlplus-ffi` and ensure it's discoverable
(`%PATH%` / `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH`), or set:

```pwsh
$env:TOMLPLUS_LIB = "C:\path\to\tomlplus_ffi.dll"
```

## Usage

```ruby
require "tomlplus"

doc = Tomlplus.loads(<<~SRC)
  [server]
  port = 8080
SRC

doc.resolve("server.port")   # => 8080
doc.has_annotation?("server.port", "type")
Tomlplus.validate(doc)
Tomlplus.dumps(doc)
```

API: `loads`, `load`, `loads_validated`, `load_validated`, `validate`,
`validate_all`, `dumps`. `Document` methods: `config`, `vars`, `meta`,
`resolve`, `has_annotation?`, `tags`, `required_keys`, `deprecated_keys`.
