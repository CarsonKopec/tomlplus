# tomlplus (.NET)

.NET bindings to `tomlplus_ffi` via P/Invoke. Targets `net8.0`.

## Install

```pwsh
dotnet add package Tomlplus
```

The runtime needs `tomlplus_ffi.dll` / `libtomlplus_ffi.so` / `libtomlplus_ffi.dylib`
on its native loader search path. Either install the shared library
system-wide or place it next to your assembly.

## Usage

```csharp
using Tomlplus;

using var doc = TomlplusApi.Parse("""
    [server]
    port = 8080
    """);

Console.WriteLine(doc.Resolve("server.port")!.Value.GetInt32());  // 8080
TomlplusApi.Validate(doc);
Console.WriteLine(TomlplusApi.Dumps(doc));
```

API: `TomlplusApi.Parse`, `TomlplusApi.Load`, `TomlplusApi.Validate`,
`TomlplusApi.Dumps`, `TomlplusApi.Version`. `Document` is `IDisposable`.
