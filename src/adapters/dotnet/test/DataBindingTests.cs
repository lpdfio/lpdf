using Lpdf.Engine;
using Xunit;

namespace Lpdf.Tests;

/// <summary>
/// Integration tests for data binding via <c>data-*</c> XML attributes.
/// These tests pass a data object to <see cref="LpdfEngine.RenderPdf(string, RenderOptions?)"/>
/// and verify the output is a valid PDF byte stream.
/// </summary>
public class DataBindingTests
{
    private static string Doc(string body) =>
        $"""<lpdf version="1"><document><section><layout>{body}</layout></section></document></lpdf>""";

    [Fact]
    public async Task DataValue_SubstitutesScalar()
    {
        var xml = Doc("""<text data-value="name">Fallback</text>""");
        using var engine = new LpdfEngine("test-key");
        var bytes = await engine.RenderPdf(xml, new RenderOptions
        {
            Data = new { name = "Acme Inc" },
        });
        Assert.StartsWith("%PDF-", System.Text.Encoding.Latin1.GetString(bytes[..5]));
    }

    [Fact]
    public async Task DataSource_ExpandsArray()
    {
        var xml = Doc("""
            <stack data-source="items" gap="xs">
              <text data-value="label">Fallback item</text>
            </stack>
            """);
        using var engine = new LpdfEngine("test-key");
        var bytes = await engine.RenderPdf(xml, new RenderOptions
        {
            Data = new
            {
                items = new[]
                {
                    new { label = "Alpha" },
                    new { label = "Beta" },
                    new { label = "Gamma" },
                },
            },
        });
        Assert.StartsWith("%PDF-", System.Text.Encoding.Latin1.GetString(bytes[..5]));
    }

    [Fact]
    public async Task DataIf_HidesNodeWhenFalse()
    {
        var xml = Doc("""
            <text data-if="isPremium">Premium only</text>
            <text>Always visible</text>
            """);
        using var engine = new LpdfEngine("test-key");
        var bytes = await engine.RenderPdf(xml, new RenderOptions
        {
            Data = new { isPremium = false },
        });
        Assert.StartsWith("%PDF-", System.Text.Encoding.Latin1.GetString(bytes[..5]));
    }

    [Fact]
    public async Task NoDataOption_RendersWithFallbackContent()
    {
        var xml = Doc("""<text data-value="name">Inline fallback</text>""");
        using var engine = new LpdfEngine("test-key");
        var bytes = await engine.RenderPdf(xml);
        Assert.StartsWith("%PDF-", System.Text.Encoding.Latin1.GetString(bytes[..5]));
    }
}
