using System.Text.Json;
using Tomlplus;
using Xunit;

public class TomlplusTests
{
    [Fact]
    public void ParseAndResolve()
    {
        using var doc = TomlplusApi.Parse("""
            [server]
            @type: int
            @min: 1
            @max: 65535
            port = 8080
            """);
        var port = doc.Resolve("server.port");
        Assert.NotNull(port);
        Assert.Equal(8080, port!.Value.GetInt32());
        Assert.True(doc.HasAnnotation("server.port", "type"));
    }

    [Fact]
    public void ValidateThrowsOnBadValue()
    {
        using var doc = TomlplusApi.Parse("@min: 100\nport = 1");
        Assert.Throws<TomlplusException>(() => TomlplusApi.Validate(doc));
    }

    [Fact]
    public void DumpsRoundTrip()
    {
        using var doc  = TomlplusApi.Parse("[a]\nx = 1");
        using var doc2 = TomlplusApi.Parse(TomlplusApi.Dumps(doc));
        Assert.Equal(1, doc2.Resolve("a.x")!.Value.GetInt32());
    }

    [Fact]
    public void VarsAndExpressions()
    {
        using var doc = TomlplusApi.Parse("""
            [vars]
            base = "https://api"
            [svc]
            url = $base + "/v1"
            """);
        Assert.Equal("https://api/v1", doc.Resolve("svc.url")!.Value.GetString());
    }
}
