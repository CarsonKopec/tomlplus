# tomlplus (Python)

Python bindings for the TOML+ language core — same API as the pure-Python
`tomlplus` package, but the parser/validator/serialiser is the Rust core
shared with the LSP server.

## Install (from source)

```pwsh
pip install maturin
cd crates/tomlplus-python
maturin develop --release      # installs into the active env
```

`maturin develop` compiles the Rust crate and installs a wheel into the
currently-active Python environment. Use `maturin build --release` to
produce a redistributable `.whl` in `target/wheels/`.

## Usage

Identical to the original package:

```python
import tomlplus

doc = tomlplus.loads('''
[server]
@type: int
@min: 1
@max: 65535
port = $ENV.PORT ?? 8080
''')

doc["server"]["port"]                  # 8080
doc.resolve("server.port")             # 8080
doc.has_annotation("server.port", "type")  # True
tomlplus.validate(doc)                 # raises ValidationError on failure
print(tomlplus.dumps(doc))             # round-trip back to TOML+
```

## API

* `loads(source) -> TOMLPlusDocument`
* `load(path) -> TOMLPlusDocument`
* `loads_validated(source) -> TOMLPlusDocument`
* `load_validated(path) -> TOMLPlusDocument`
* `dumps(doc_or_dict) -> str`
* `validate(doc) -> None`         — raises on first failure
* `validate_all(doc) -> list[ValidationError]`
* Classes: `TOMLPlusDocument`, `Annotation`
* Exceptions: `TOMLPlusError`, `ParseError`, `ValidationError`, `VariableError`
