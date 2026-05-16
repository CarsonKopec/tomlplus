// .NET harness — parses a .tomlp file via P/Invoke, prints JSON.

using System.Text.Json;
using Tomlplus;

if (args.Length != 1)
{
    Console.Error.WriteLine("usage: harness <fixture.tomlp>");
    return 1;
}

var src = File.ReadAllText(args[0]);
using var doc = TomlplusApi.Parse(src);

var json = doc.Config().GetRawText();
// Re-pretty-print to normalise indentation across bindings.
using var parsed = JsonDocument.Parse(json);
var opts = new JsonSerializerOptions { WriteIndented = true };
Console.WriteLine(JsonSerializer.Serialize(parsed.RootElement, opts));
return 0;
