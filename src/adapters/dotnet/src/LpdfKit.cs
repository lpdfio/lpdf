using Lpdf.Canvas;
using Lpdf.Kit;
using Lpdf.Layout;
using Lpdf.Shared;

namespace Lpdf;

/// <summary>
/// Static document skeleton builder.
/// Produces <see cref="Document"/>, <see cref="SectionNode"/>,
/// <see cref="SectionLayout"/>, and <see cref="SectionCanvas"/> instances
/// for use with <see cref="LpdfEngine"/>.
///
/// <example>
/// <code>
/// using Lpdf;
/// using Lpdf.Kit;
/// using Lpdf.Layout;
///
/// var doc = LpdfKit.Document(
///     nodes: [
///         LpdfKit.Section(
///             nodes: [ LpdfKit.Layout([ LpdfLayout.Text(["Hello"]) ]) ],
///             options: new SectionOptions(Size: "a4"))
///     ]);
/// </code>
/// </example>
/// </summary>
public static class LpdfKit
{
    // ── Section ───────────────────────────────────────────────────────────────

    /// <summary>Build a <c>section</c> (page) node.</summary>
    public static SectionNode Section(SectionContent[]? nodes = null, SectionOptions? options = null)
        => new(AttrsHelper.Attrs(options), (nodes ?? []).ToList());

    /// <summary>
    /// Wrap layout nodes into a <c>layout</c> block for use inside a <see cref="Section"/>.
    /// </summary>
    public static SectionLayout Layout(Node[]? nodes = null)
        => new((nodes ?? []).ToList(), new Dictionary<string, string>(StringComparer.Ordinal));

    /// <summary>
    /// Wrap canvas layer nodes into a <c>canvas</c> block for use inside a <see cref="Section"/>.
    /// </summary>
    public static SectionCanvas Canvas(LayerNode[]? layers = null)
        => new((layers ?? []).ToList(), new Dictionary<string, string>(StringComparer.Ordinal));

    // ── Document ──────────────────────────────────────────────────────────────

    /// <summary>Build the root <c>document</c> node, ready for <see cref="LpdfEngine.RenderPdf(Document, Engine.RenderOptions?)"/>.</summary>
    public static Document Document(SectionNode[]? nodes = null, DocumentOptions? options = null)
    {
        var opts  = options ?? new DocumentOptions();
        var attrs = new Dictionary<string, object?>(StringComparer.Ordinal);

        if (opts.Size        is not null) attrs["size"]        = opts.Size;
        if (opts.Orientation is not null) attrs["orientation"] = opts.Orientation;
        if (opts.Margin      is not null) attrs["margin"]      = opts.Margin;
        if (opts.Background  is not null) attrs["background"]  = opts.Background;

        if (opts.Tokens is not null)
            attrs["tokens"] = SerializeTokens(opts.Tokens);

        if (opts.Meta is not null)
            attrs["meta"] = SerializeMeta(opts.Meta);

        return new Document(attrs, (nodes ?? []).ToList());
    }

    // ── Private ───────────────────────────────────────────────────────────────

    private static Dictionary<string, object?> SerializeTokens(DocumentTokens t)
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
                    FontSrc     src     => (object)new { src     = src.Src },
                    FontBuiltin builtin => (object)new { builtin = builtin.Builtin },
                    _                   => null,
                };
            d["fonts"] = fonts;
        }
        return d;
    }

    private static Dictionary<string, string?> SerializeMeta(DocumentMeta m) =>
        new(StringComparer.Ordinal)
        {
            ["title"]    = m.Title,
            ["author"]   = m.Author,
            ["subject"]  = m.Subject,
            ["keywords"] = m.Keywords,
            ["creator"]  = m.Creator,
        };
}

