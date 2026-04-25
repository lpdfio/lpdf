using Lpdf.Kit;
using Lpdf.Layout;
using Lpdf.Shared;

namespace Lpdf;

/// <summary>
/// Static layout node builder helpers.
/// All methods return <see cref="Layout.Node"/> instances.
///
/// <example>
/// <code>
/// using Lpdf;
/// using Lpdf.Layout;
///
/// var text = LpdfLayout.Text(["Hello"], new TextOptions(Font: "heading"));
/// var stack = LpdfLayout.Stack([text], new StackOptions(Gap: "m"));
/// </code>
/// </example>
/// </summary>
public static class LpdfLayout
{
    // ── Container helpers ─────────────────────────────────────────────────────

    /// <summary>Build a <c>stack</c> layout node (vertical column).</summary>
    public static ContainerNode Stack(Node[]? nodes = null, StackOptions? options = null)
        => Container("stack", options, nodes);

    /// <summary>Build a <c>flank</c> layout node (horizontal row).</summary>
    public static ContainerNode Flank(Node[]? nodes = null, FlankOptions? options = null)
        => Container("flank", options, nodes);

    /// <summary>Build a <c>split</c> layout node (two-column split).</summary>
    public static ContainerNode Split(Node[]? nodes = null, SplitOptions? options = null)
        => Container("split", options, nodes);

    /// <summary>Build a <c>cluster</c> layout node (wrapping flex row).</summary>
    public static ContainerNode Cluster(Node[]? nodes = null, ClusterOptions? options = null)
        => Container("cluster", options, nodes);

    /// <summary>Build a <c>grid</c> layout node (multi-column grid).</summary>
    public static ContainerNode Grid(Node[]? nodes = null, GridOptions? options = null)
        => Container("grid", options, nodes);

    /// <summary>Build a <c>frame</c> layout node (fixed-size container).</summary>
    public static ContainerNode Frame(Node[]? nodes = null, FrameOptions? options = null)
        => Container("frame", options, nodes);

    /// <summary>Build a <c>link</c> layout node (hyperlink wrapper).</summary>
    public static ContainerNode Link(Node[]? nodes = null, LinkOptions? options = null)
        => Container("link", options, nodes);

    // ── Table helpers ─────────────────────────────────────────────────────────

    /// <summary>Build a <c>table</c> layout node.</summary>
    public static ContainerNode Table(Node[]? nodes = null, TableOptions? options = null)
        => Container("table", options, nodes);

    /// <summary>Build a <c>thead</c> table header row group.</summary>
    public static ContainerNode Thead(Node[]? nodes = null, TheadOptions? options = null)
        => Container("thead", options, nodes);

    /// <summary>Build a <c>tr</c> table row.</summary>
    public static ContainerNode Tr(Node[]? nodes = null, TrOptions? options = null)
        => Container("tr", options, nodes);

    /// <summary>Build a <c>td</c> table cell.</summary>
    public static ContainerNode Td(Node[]? nodes = null, TdOptions? options = null)
        => Container("td", options, nodes);

    // ── Leaf helpers ──────────────────────────────────────────────────────────

    /// <summary>Build a <c>divider</c> horizontal rule node.</summary>
    public static DividerNode Divider(DividerOptions? options = null)
        => new(AttrsHelper.Attrs(options));

    /// <summary>Build an <c>img</c> image node.</summary>
    public static ImgNode Img(ImgOptions options)
        => new(AttrsHelper.Attrs(options));

    /// <summary>Build a <c>barcode</c> node.</summary>
    public static BarcodeNode Barcode(BarcodeOptions options)
        => new(AttrsHelper.Attrs(options));

    // ── Text helpers ──────────────────────────────────────────────────────────

    /// <summary>Build a <c>text</c> paragraph node.</summary>
    public static TextNode Text(Content[]? nodes = null, TextOptions? options = null) => new(
        AttrsHelper.Attrs(options),
        (nodes ?? []).ToList());

    /// <summary>Wrap a plain string as inline <see cref="Content"/> for use in <see cref="Text(Content[], TextOptions)"/>.</summary>
    public static Content Text(string raw) => new RawText(raw);

    /// <summary>Build a <c>span</c> inline node.</summary>
    public static SpanNode Span(string[]? nodes = null, SpanOptions? options = null) => new(
        AttrsHelper.Attrs(options),
        (nodes ?? []).ToList());

    // ── Region / Field ────────────────────────────────────────────────────────

    /// <summary>Build a <c>region</c> node.</summary>
    public static RegionNode Region(Node[]? nodes = null, RegionOptions? options = null)
        => new(AttrsHelper.Attrs(options), (nodes ?? []).ToList());

    /// <summary>Build a <c>field</c> form node.</summary>
    public static FieldNode Field(FieldOptions options)
        => new(AttrsHelper.Attrs(options));

    // ── Private ───────────────────────────────────────────────────────────────

    private static ContainerNode Container(string type, object? options, Node[]? children)
        => new(AttrsHelper.Attrs(options), (children ?? []).ToList(), type);
}
