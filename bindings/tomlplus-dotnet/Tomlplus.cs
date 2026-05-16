// TOML+ .NET bindings — P/Invoke over `tomlplus_ffi`.

using System.Runtime.InteropServices;
using System.Text.Json;

namespace Tomlplus;

internal static partial class Native
{
    private const string LibName = "tomlplus_ffi";

    [LibraryImport(LibName, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr tomlplus_parse(string source);

    [LibraryImport(LibName)] internal static partial IntPtr tomlplus_to_json(IntPtr doc);
    [LibraryImport(LibName)] internal static partial IntPtr tomlplus_meta_json(IntPtr doc);
    [LibraryImport(LibName)] internal static partial IntPtr tomlplus_vars_json(IntPtr doc);
    [LibraryImport(LibName)] internal static partial IntPtr tomlplus_validate(IntPtr doc);
    [LibraryImport(LibName)] internal static partial IntPtr tomlplus_dumps(IntPtr doc);
    [LibraryImport(LibName)] internal static partial void   tomlplus_free(IntPtr doc);
    [LibraryImport(LibName)] internal static partial void   tomlplus_free_string(IntPtr s);
    [LibraryImport(LibName)] internal static partial IntPtr tomlplus_last_error();
    [LibraryImport(LibName)] internal static partial IntPtr tomlplus_version();

    internal static string LastError()
        => Marshal.PtrToStringUTF8(tomlplus_last_error()) ?? "unknown FFI error";

    internal static string TakeString(IntPtr p)
    {
        if (p == IntPtr.Zero)
            throw new TomlplusException(LastError());
        try   { return Marshal.PtrToStringUTF8(p) ?? string.Empty; }
        finally { tomlplus_free_string(p); }
    }
}

/// <summary>Thrown for parse failures, validation errors, and FFI errors.</summary>
public sealed class TomlplusException : Exception
{
    public TomlplusException(string message) : base(message) {}
}

/// <summary>A `@name: arg` annotation attached to a key.</summary>
public sealed record Annotation(string Name, object? Arg);

/// <summary>A parsed TOML+ document. Implements <see cref="IDisposable"/>.</summary>
public sealed class Document : IDisposable
{
    private IntPtr _handle;
    private readonly object _gate = new();

    internal Document(IntPtr handle) => _handle = handle;

    internal IntPtr Handle
    {
        get
        {
            if (_handle == IntPtr.Zero)
                throw new ObjectDisposedException(nameof(Document));
            return _handle;
        }
    }

    /// <summary>Whole config tree as a <see cref="JsonElement"/>.</summary>
    public JsonElement Config()
        => JsonDocument.Parse(Native.TakeString(Native.tomlplus_to_json(Handle))).RootElement;

    /// <summary>Resolved <c>[vars]</c> section.</summary>
    public JsonElement Vars()
        => JsonDocument.Parse(Native.TakeString(Native.tomlplus_vars_json(Handle))).RootElement;

    /// <summary>Annotation metadata, keyed by dotted path.</summary>
    public Dictionary<string, List<Annotation>> Meta()
    {
        var s = Native.TakeString(Native.tomlplus_meta_json(Handle));
        using var doc = JsonDocument.Parse(s);
        var result = new Dictionary<string, List<Annotation>>();
        foreach (var prop in doc.RootElement.EnumerateObject())
        {
            var list = new List<Annotation>();
            foreach (var elt in prop.Value.EnumerateArray())
            {
                var name = elt.GetProperty("name").GetString() ?? string.Empty;
                object? arg = elt.GetProperty("arg").ValueKind switch
                {
                    JsonValueKind.String => elt.GetProperty("arg").GetString(),
                    JsonValueKind.Number => elt.GetProperty("arg").GetDouble(),
                    JsonValueKind.Array  => elt.GetProperty("arg")
                                              .EnumerateArray()
                                              .Select(e => e.GetString())
                                              .ToList(),
                    JsonValueKind.True   => true,
                    JsonValueKind.False  => false,
                    _ => null,
                };
                list.Add(new Annotation(name, arg));
            }
            result[prop.Name] = list;
        }
        return result;
    }

    /// <summary>Walk a dotted path; returns <c>null</c> if any segment is missing.</summary>
    public JsonElement? Resolve(string path)
    {
        var node = Config();
        foreach (var part in path.Split('.'))
        {
            if (node.ValueKind != JsonValueKind.Object) return null;
            if (!node.TryGetProperty(part, out var next)) return null;
            node = next;
        }
        return node;
    }

    public bool HasAnnotation(string path, string name)
        => Meta().TryGetValue(path, out var anns)
            && anns.Any(a => a.Name == name);

    /// <summary>Return every validation diagnostic for this document.</summary>
    public List<ValidationError> ValidateAll()
    {
        var s = Native.TakeString(Native.tomlplus_validate(Handle));
        return JsonSerializer.Deserialize<List<ValidationError>>(s, JsonOpts) ?? new();
    }

    private static readonly JsonSerializerOptions JsonOpts =
        new() { PropertyNameCaseInsensitive = true };

    public void Dispose()
    {
        lock (_gate)
        {
            if (_handle != IntPtr.Zero)
            {
                Native.tomlplus_free(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}

public sealed class ValidationError
{
    public string  Message  { get; set; } = "";
    public string  Severity { get; set; } = "";
    public Span    Span     { get; set; } = new();
}

public sealed class Span
{
    public int Start { get; set; }
    public int End   { get; set; }
}

/// <summary>Top-level entry points.</summary>
public static class TomlplusApi
{
    public static string Version()
        => Marshal.PtrToStringUTF8(Native.tomlplus_version()) ?? "";

    /// <summary>Parse a TOML+ source string.</summary>
    public static Document Parse(string source)
    {
        var h = Native.tomlplus_parse(source);
        if (h == IntPtr.Zero)
            throw new TomlplusException(Native.LastError());
        return new Document(h);
    }

    public static Document Load(string path) => Parse(File.ReadAllText(path));

    /// <summary>Validate; throws on the first failing constraint.</summary>
    public static void Validate(Document doc)
    {
        foreach (var e in doc.ValidateAll())
            if (e.Severity == "error")
                throw new TomlplusException(e.Message);
    }

    public static string Dumps(Document doc)
        => Native.TakeString(Native.tomlplus_dumps(doc.Handle));
}
