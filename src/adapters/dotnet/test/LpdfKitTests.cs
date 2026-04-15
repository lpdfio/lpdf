using Lpdf;
using Xunit;

namespace Lpdf.Tests;

public class LpdfKitTypesTests
{
    // ── LpdfDocument structure ────────────────────────────────────────────────

    [Fact]
    public void Document_HasVersion1AndTypeDocument()
    {
        var doc = LpdfKit.Document(new DocumentInput());
        Assert.Equal(1,          doc.Version);
        Assert.Equal("document", doc.Type);
    }

    [Fact]
    public void Document_MetaAndTokensInAttrs()
    {
        var doc = LpdfKit.Document(new DocumentInput
        {
            Options = new DocumentOptions
            {
                Meta   = new LpdfMeta(Title: "Test"),
                Tokens = new LpdfTokens(Colors: new() { ["primary"] = "#ff0000" }),
            },
        });

        Assert.True(doc.Attrs.ContainsKey("meta"));
        Assert.True(doc.Attrs.ContainsKey("tokens"));
    }

    [Fact]
    public void Document_StringOptionsAppearedInAttrs()
    {
        var doc = LpdfKit.Document(new DocumentInput
        {
            Options = new DocumentOptions(Size: "a4", Margin: "28pt"),
        });
        Assert.Equal("a4",   doc.Attrs["size"]);
        Assert.Equal("28pt", doc.Attrs["margin"]);
    }

    // ── LpdfKit helpers ───────────────────────────────────────────────────────

    [Fact]
    public void Stack_ProducesContainerNodeWithType()
    {
        var node = LpdfKit.Stack(new StackInput
        {
            Options = new StackOptions(Gap: "m", Background: "surface"),
        });
        var container = Assert.IsType<LpdfContainerNode>(node);
        Assert.Equal("stack", container.Type);
        Assert.Equal("m",       container.Attrs["gap"]);
        Assert.Equal("surface", container.Attrs["background"]);
    }

    [Fact]
    public void Grid_ColWidthKebabCased()
    {
        var node = (LpdfContainerNode)LpdfKit.Grid(new GridInput
        {
            Options = new GridOptions(ColWidth: "120pt", Cols: "3"),
        });
        Assert.Equal("120pt", node.Attrs["col-width"]);
        Assert.Equal("3",     node.Attrs["cols"]);
    }

    [Fact]
    public void Text_AcceptsRawStringsAndSpans()
    {
        var node = LpdfKit.Text(new TextInput
        {
            Nodes = [
                new LpdfRawText("Total: "),   // explicit
                LpdfKit.Span(new SpanInput
                {
                    Nodes   = ["$100"],
                    Options = new SpanOptions(Bold: "true", Color: "primary"),
                }),
            ],
        });
        Assert.Equal(2, node.Children.Count);
        Assert.IsType<LpdfRawText>(node.Children[0]);
        var span = Assert.IsType<LpdfSpanNode>(node.Children[1]);
        Assert.Equal("true",    span.Attrs["bold"]);
        Assert.Equal("primary", span.Attrs["color"]);
    }

    [Fact]
    public void Divider_HasNoChildren()
    {
        var node = LpdfKit.Divider(new DividerInput
        {
            Options = new DividerOptions(Color: "surface-alt"),
        });
        Assert.Equal("divider",     node.Type);
        Assert.Equal("surface-alt", node.Attrs["color"]);
    }

    [Fact]
    public void Page_ChildrenAreLayoutNodes()
    {
        var page = LpdfKit.Page(new PageInput
        {
            Nodes = [LpdfKit.Stack(new StackInput())],
            Options = new PageOptions(Size: "a4"),
        });
        Assert.Equal("page", page.Type);
        Assert.Equal("a4",   page.Attrs["size"]);
        Assert.Single(page.Children);
    }
}
