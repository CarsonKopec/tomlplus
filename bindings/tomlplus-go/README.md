# tomlplus-go

Go bindings for the TOML+ C library (`tomlplus_ffi`).

## Install

```pwsh
go get github.com/CarsonKopec/tomlplus/bindings/tomlplus-go
```

The cgo build needs the `tomlplus_ffi` shared library and `tomlplus.h` on
the include / library search paths. After building the workspace
(`cargo build --release -p tomlplus-ffi`):

```pwsh
# Windows (PowerShell)
$env:CGO_CFLAGS  = "-I$PWD\crates\tomlplus-ffi\include"
$env:CGO_LDFLAGS = "-L$PWD\target\release"
go test .

# Linux / macOS (bash)
export CGO_CFLAGS="-I$PWD/crates/tomlplus-ffi/include"
export CGO_LDFLAGS="-L$PWD/target/release"
go test .
```

At runtime, `tomlplus_ffi.dll` (or `.so` / `.dylib`) must be on the loader
search path (Windows: `%PATH%`; Linux: `$LD_LIBRARY_PATH`; macOS: `$DYLD_LIBRARY_PATH`).

## Usage

```go
import "github.com/CarsonKopec/tomlplus/bindings/tomlplus-go"

doc, err := tomlplus.Parse(`[server]
port = 8080`)
if err != nil { panic(err) }
defer doc.Close()

v, _ := doc.Resolve("server.port")
fmt.Println(v) // 8080
```

API: `Parse`, `Load`, `Validate`, `ValidateAll`, `Dumps`, `Version`.
Document methods: `Config`, `Vars`, `Meta`, `Resolve`, `Close`.
