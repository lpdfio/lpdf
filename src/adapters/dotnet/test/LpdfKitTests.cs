using Lpdf;
using Lpdf.Engine;
using Lpdf.Kit;
using Lpdf.Layout;
using Xunit;

namespace Lpdf.Tests;

public class LpdfKitTypesTests
{
    // -- Document structure ----------------------------------------------------

    [Fact]
    public void Document_HasVersion1AndTypeDocument()
    {
        var doc = LpdfKit.Document();
        Assert.Equal(1,          doc.Version);
        Assert.Equal("document", doc.Type);
    }

    [Fact]
    public void Document_MetaAndTokensInAttrs()
    {
        var doc = LpdfKit.Document(options: new DocumentOptions(
            Meta:   new DocumentMeta(Title: "Test"),
            Tokens: new DocumentTokens(Colors: new() { ["primary"] = "#ff0000" })
        ));

        Assert.True(doc.Attrs.ContainsKey("meta"));
        Assert.True(doc.Attrs.ContainsKey("tokens"));
    }

    [Fact]
    public void Document_StringOptionsAppearedInAttrs()
    {
        var doc = LpdfKit.Document(options: new DocumentOptions(Size: "a4", Margin: "28pt"));
        Assert.Equal("a4",   doc.Attrs["size"]);
        Assert.Equal("28pt", doc.Attrs["margin"]);
    }

    // -- LpdfLayout helpers ----------------------------------------------------

    [Fact]
    public void Stack_ProducesContainerNodeWithType()
    {
        var node = LpdfLayout.Stack(options: new StackOptions(Gap: "m", Background: "surface"));
        var container = Assert.IsType<ContainerNode>(node);
        Assert.Equal("stack", container.Type);
        Assert.Equal("m",       container.Attrs["gap"]);
        Assert.Equal("surface", container.Attrs["background"]);
    }

    [Fact]
    public void Grid_ColWidthKebabCased()
    {
        var node = (ContainerNode)LpdfLayout.Grid(options: new GridOptions(ColWidth: "120pt", Cols: "3"));
        Assert.Equal("120pt", node.Attrs["col-width"]);
        Assert.Equal("3",     node.Attrs["cols"]);
    }

    [Fact]
    public void Text_AcceptsRawStringsAndSpans()
    {
        var raw  = LpdfLayout.Text("Total: ");
        var span = LpdfLayout.Span(
            nodes:   ["$100"],
            options: new SpanOptions(Bold: "true", Color: "primary"));

        var node = LpdfLayout.Text(nodes: [raw, span]);
        Assert.Equal(2, node.Children.Count);
        Assert.False(node.Children[0] is SpanNode);
        var s = Assert.IsType<SpanNode>(node.Children[1]);
        Assert.Equal("true",    s.Attrs["bold"]);
        Assert.Equal("primary", s.Attrs["color"]);
    }

    [Fact]
    public void Divider_HasNoChildren()
    {
        var node = LpdfLayout.Divider(options: new DividerOptions(Color: "surface-alt"));
        Assert.Equal("divider",     node.Type);
        Assert.Equal("surface-alt", node.Attrs["color"]);
    }

    [Fact]
    public void Section_ChildrenAreLayoutNodes()
    {
        var layout = LpdfKit.Layout(nodes: [LpdfLayout.Stack()]);
        var section = LpdfKit.Section(
            nodes:   [layout],
            options: new SectionOptions(Size: "a4"));
        Assert.Equal("section", section.Type);
        Assert.Equal("a4",      section.Attrs["size"]);
        Assert.Single(layout.Nodes);
    }
}

// -- KitToXml tests ------------------------------------------------------------

public class KitToXmlTests
{
    private static Document SimpleDoc() =>
        LpdfKit.Document(nodes: [LpdfKit.Section(nodes: [LpdfKit.Layout()])]);

    [Fact]
    public async Task KitToXml_ReturnsXmlDeclaration()
    {
        var engine = new LpdfEngine("test-key");
        var xml    = await engine.KitToXml(SimpleDoc());
        Assert.StartsWith("<?xml version=\"1.0\"", xml);
    }

    [Fact]
    public async Task KitToXml_ContainsLpdfRoot()
    {
        var engine = new LpdfEngine("test-key");
        var xml    = await engine.KitToXml(SimpleDoc());
        Assert.Contains("<lpdf version=\"1\">", xml);
    }

    [Fact]
    public async Task KitToXml_BuiltinFontPlacedInAssets()
    {
        var doc = LpdfKit.Document(
            nodes:   [LpdfKit.Section(nodes: [LpdfKit.Layout()])],
            options: new DocumentOptions(
                Tokens: new DocumentTokens(Fonts: new()
                {
                    ["heading"] = new FontBuiltin("Helvetica-Bold"),
                })
            )
        );
        var engine = new LpdfEngine("test-key");
        var xml    = await engine.KitToXml(doc);

        Assert.Contains("<assets>",               xml);
        Assert.Contains("core=\"Helvetica-Bold\"", xml);

        var tokensStart   = xml.IndexOf("<tokens>",  StringComparison.Ordinal);
        var tokensEnd     = xml.IndexOf("</tokens>", StringComparison.Ordinal);
        var fontsInTokens = tokensStart >= 0 ? xml.IndexOf("<fonts>", tokensStart, StringComparison.Ordinal) : -1;
        Assert.True(
            tokensStart < 0 || fontsInTokens < 0 || fontsInTokens > tokensEnd,
            "Font was incorrectly placed inside <tokens>");
    }

    [Fact]
    public async Task KitToXml_CustomFontUsesRefAlias()
    {
        var doc = LpdfKit.Document(
            nodes:   [LpdfKit.Section(nodes: [LpdfKit.Layout()])],
            options: new DocumentOptions(
                Tokens: new DocumentTokens(Fonts: new()
                {
                    ["body"] = new FontSrc("/fonts/MyFont.ttf"),
                })
            )
        );
        var engine = new LpdfEngine("test-key");
        var xml    = await engine.KitToXml(doc);

        Assert.Contains("ref=\"body\"", xml);
        Assert.DoesNotContain("ref=\"/fonts/MyFont.ttf\"", xml);
        Assert.Contains("src=", xml);
    }

    [Fact]
    public async Task KitToXml_ProducedXmlRendersToValidPdf()
    {
        var doc = LpdfKit.Document(nodes: [
            LpdfKit.Section(nodes: [LpdfKit.Layout(nodes: [
                LpdfLayout.Text(nodes: [LpdfLayout.Text("Hello from KitToXml")]),
            ])]),
        ]);
        var engine = new LpdfEngine("test-key");
        var xml    = await engine.KitToXml(doc);
        var pdf    = await engine.RenderPdf(xml);

        Assert.True(pdf.Length > 100);
        Assert.Equal(0x25, pdf[0]); // '%'
        Assert.Equal(0x50, pdf[1]); // 'P'
        Assert.Equal(0x44, pdf[2]); // 'D'
        Assert.Equal(0x46, pdf[3]); // 'F'
    }
}
