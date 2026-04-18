using System.Collections.Concurrent;
using System.Reflection;

namespace Lpdf;

/// <summary>
/// Static tree-builder helpers for constructing lpdf document trees.
///
/// All helpers return plain serialisable records. Pass the result of
/// <see cref="Document"/> to <see cref="LpdfEngine.RenderPdf(LpdfDocument, RenderOptions?)"/>.
///
/// <example>
/// <code>
/// using Lpdf;
///
/// var engine = new LpdfEngine(licenseKey);
/// var bytes  = await engine.RenderPdf(
///     LpdfKit.Document(new DocumentInput {
///         Nodes = [
///             LpdfKit.Page(new PageInput {
///                 Nodes = [LpdfKit.Text(new TextInput { Nodes = ["Hello, world!"] })],
///                 Options = new PageOptions { Size = "a4", Margin = "28pt" },
///             }),
///         ],
///         Options = new DocumentOptions {
///             Meta = new LpdfMeta(Title: "My Doc"),
///         },
///     }));
/// </code>
/// </example>
/// </summary>
public static class LpdfKit
{
    // ── Container helpers ─────────────────────────────────────────────────────

    /// <summary>Build a <c>stack</c> layout node (vertical column).</summary>
    public static LpdfNode Stack(StackInput input) =>
        Container("stack", input.Options, input.Nodes);

    /// <summary>Build a <c>flank</c> layout node (horizontal row).</summary>
    public static LpdfNode Flank(FlankInput input) =>
        Container("flank", input.Options, input.Nodes);

    /// <summary>Build a <c>split</c> layout node (two-column split).</summary>
    public static LpdfNode Split(SplitInput input) =>
        Container("split", input.Options, input.Nodes);

    /// <summary>Build a <c>cluster</c> layout node (wrapping flex row).</summary>
    public static LpdfNode Cluster(ClusterInput input) =>
        Container("cluster", input.Options, input.Nodes);

    /// <summary>Build a <c>grid</c> layout node (multi-column grid).</summary>
    public static LpdfNode Grid(GridInput input) =>
        Container("grid", input.Options, input.Nodes);

    /// <summary>Build a <c>frame</c> layout node (fixed-size container).</summary>
    public static LpdfNode Frame(FrameInput input) =>
        Container("frame", input.Options, input.Nodes);

    /// <summary>Build a <c>link</c> layout node (hyperlink wrapper).</summary>
    public static LpdfNode Link(LinkInput input) =>
        Container("link", input.Options, input.Nodes);

    // ── Leaf helpers ──────────────────────────────────────────────────────────

    /// <summary>Build a <c>divider</c> horizontal rule node.</summary>
    public static LpdfDividerNode Divider(DividerInput input) =>
        new(Attrs(input.Options));

    /// <summary>Build an <c>img</c> image node.</summary>
    public static LpdfImgNode Img(ImgInput input) =>
        new(Attrs(input.Options));

    /// <summary>Build a <c>barcode</c> node.</summary>
    public static LpdfBarcodeNode Barcode(BarcodeInput input) =>
        new(Attrs(input.Options));

    // ── Table helpers ─────────────────────────────────────────────────────────

    /// <summary>Build a <c>table</c> layout node.</summary>
    public static LpdfContainerNode Table(TableInput input) =>
        Container("table", input.Options, input.Nodes);

    /// <summary>Build a <c>thead</c> table header row group.</summary>
    public static LpdfContainerNode Thead(TheadInput input) =>
        Container("thead", input.Options, input.Nodes);

    /// <summary>Build a <c>tr</c> table row.</summary>
    public static LpdfContainerNode Tr(TrInput input) =>
        Container("tr", input.Options, input.Nodes);

    /// <summary>Build a <c>td</c> table cell.</summary>
    public static LpdfContainerNode Td(TdInput input) =>
        Container("td", input.Options, input.Nodes);

    // ── Text helpers ──────────────────────────────────────────────────────────

    /// <summary>Build a <c>text</c> paragraph node.</summary>
    public static LpdfTextNode Text(TextInput input) => new(
        Attrs(input.Options),
        (input.Nodes ?? []).ToList());

    /// <summary>Build a <c>span</c> inline node.</summary>
    public static LpdfSpanNode Span(SpanInput input) => new(
        Attrs(input.Options),
        (input.Nodes ?? []).ToList());

    // ── Page + document ───────────────────────────────────────────────────────

    /// <summary>Build a <c>page</c> node.</summary>
    public static LpdfPageNode Page(PageInput input) => new(
        Attrs(input.Options),
        (input.Nodes ?? []).ToList());

    /// <summary>Build the root <c>document</c> node, ready for <see cref="LpdfEngine.RenderPdf(LpdfDocument, RenderOptions?)"/>.</summary>
    public static LpdfDocument Document(DocumentInput input)
    {
        var opts   = input.Options ?? new DocumentOptions();
        var attrs  = new Dictionary<string, object?>(StringComparer.Ordinal);

        // Flat string attrs
        if (opts.Size        is not null) attrs["size"]        = opts.Size;
        if (opts.Orientation is not null) attrs["orientation"] = opts.Orientation;
        if (opts.Margin      is not null) attrs["margin"]      = opts.Margin;
        if (opts.Background  is not null) attrs["background"]  = opts.Background;

        // Tokens sub-object
        if (opts.Tokens is not null)
            attrs["tokens"] = SerializeTokens(opts.Tokens);

        // Meta sub-object
        if (opts.Meta is not null)
            attrs["meta"] = SerializeMeta(opts.Meta);

        return new LpdfDocument(attrs, (input.Nodes ?? []).ToList());
    }

    // ── Implicit conversion helper ────────────────────────────────────────────

    /// <summary>
    /// Wrap a plain string as <see cref="LpdfContent"/> for use in
    /// <see cref="TextInput.Nodes"/>.
    /// </summary>
    public static LpdfContent Text(string raw) => new LpdfRawText(raw);

    // ── Private helpers ───────────────────────────────────────────────────────

    private static LpdfContainerNode Container(string type, object? options, LpdfNode[]? children) =>
        new(Attrs(options), (children ?? []).ToList(), type);

    /// <summary>
    /// Reflect over any options record and convert its non-null properties to
    /// a kebab-case attribute dictionary.
    /// </summary>
    private static readonly ConcurrentDictionary<Type, PropertyInfo[]> _propCache = new();

    private static Dictionary<string, string> Attrs(object? options)
    {
        var result = new Dictionary<string, string>(StringComparer.Ordinal);
        if (options is null) return result;

        var props = _propCache.GetOrAdd(
            options.GetType(),
            t => t.GetProperties(BindingFlags.Public | BindingFlags.Instance));
        foreach (var prop in props)
        {
            var value = prop.GetValue(options);
            if (value is string s)
                result[PascalToKebab(prop.Name)] = s;
        }
        return result;
    }

    private static Dictionary<string, object?> SerializeTokens(LpdfTokens t)
    {
        var d = new Dictionary<string, object?>(StringComparer.Ordinal);
        if (t.Colors is not null) d["colors"] = t.Colors;
        if (t.Space  is not null) d["space"]  = t.Space;
        if (t.Grid   is not null) d["grid"]   = t.Grid;
        if (t.Border is not null) d["border"] = t.Border;
        if (t.Radius is not null) d["radius"] = t.Radius;
        if (t.Width  is not null) d["width"]  = t.Width;
        if (t.Text   is not null) d["text"]   = t.Text;
        if (t.Fonts  is not null)
        {
            var fonts = new Dictionary<string, object?>(StringComparer.Ordinal);
            foreach (var (name, def) in t.Fonts)
                fonts[name] = def switch
                {
                    LpdfFontSrc     src     => (object)new { src     = src.Src },
                    LpdfFontBuiltin builtin => (object)new { builtin = builtin.Builtin },
                    _                       => null,
                };
            d["fonts"] = fonts;
        }
        return d;
    }

    private static Dictionary<string, string?> SerializeMeta(LpdfMeta m) =>
        new(StringComparer.Ordinal)
        {
            ["title"]    = m.Title,
            ["author"]   = m.Author,
            ["subject"]  = m.Subject,
            ["keywords"] = m.Keywords,
            ["creator"]  = m.Creator,
        };

    /// <summary>PascalCase → kebab-case (e.g. <c>FontSize</c> → <c>font-size</c>).</summary>
    private static string PascalToKebab(string name)
    {
        var sb = new System.Text.StringBuilder(name.Length + 4);
        for (var i = 0; i < name.Length; i++)
        {
            var c = name[i];
            if (char.IsUpper(c) && i > 0) sb.Append('-');
            sb.Append(char.ToLowerInvariant(c));
        }
        return sb.ToString();
    }
}
