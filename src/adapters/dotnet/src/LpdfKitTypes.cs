using System.Text.Json;
using System.Text.Json.Serialization;

// Synthesised record constructors and positional properties are self-describing;
// suppress missing-XML-comment warnings for those members project-wide in this file.
#pragma warning disable CS1591

namespace Lpdf;

// ──────────────────────────────────────────────────────────────────────────────
// Input records — what callers pass to LpdfKit helpers
// ──────────────────────────────────────────────────────────────────────────────

/// <summary>Input to <see cref="LpdfKit.Stack"/>.</summary>
public sealed record StackInput   (LpdfNode[]?     Nodes = null, StackOptions?    Options = null);
/// <summary>Input to <see cref="LpdfKit.Flank"/>.</summary>
public sealed record FlankInput   (LpdfNode[]?     Nodes = null, FlankOptions?    Options = null);
/// <summary>Input to <see cref="LpdfKit.Split"/>.</summary>
public sealed record SplitInput   (LpdfNode[]?     Nodes = null, SplitOptions?    Options = null);
/// <summary>Input to <see cref="LpdfKit.Cluster"/>.</summary>
public sealed record ClusterInput (LpdfNode[]?     Nodes = null, ClusterOptions?  Options = null);
/// <summary>Input to <see cref="LpdfKit.Grid"/>.</summary>
public sealed record GridInput    (LpdfNode[]?     Nodes = null, GridOptions?     Options = null);
/// <summary>Input to <see cref="LpdfKit.Frame"/>.</summary>
public sealed record FrameInput   (LpdfNode[]?     Nodes = null, FrameOptions?    Options = null);
/// <summary>Input to <see cref="LpdfKit.Link"/>.</summary>
public sealed record LinkInput    (LpdfNode[]?     Nodes = null, LinkOptions?     Options = null);
/// <summary>Input to <see cref="LpdfKit.Text(TextInput)"/>.</summary>
public sealed record TextInput    (LpdfContent[]?  Nodes = null, TextOptions?     Options = null);
/// <summary>Input to <see cref="LpdfKit.Span"/>.</summary>
public sealed record SpanInput    (string[]?       Nodes = null, SpanOptions?     Options = null);
/// <summary>Input to <see cref="LpdfKit.Divider"/>.</summary>
public sealed record DividerInput (                              DividerOptions?  Options = null);
/// <summary>Input to <see cref="LpdfKit.Img"/>.</summary>
public sealed record ImgInput     (                              ImgOptions       Options);
/// <summary>Input to <see cref="LpdfKit.Barcode"/>.</summary>
public sealed record BarcodeInput (                              BarcodeOptions   Options);
/// <summary>Input to <see cref="LpdfKit.Table"/>.</summary>
public sealed record TableInput   (LpdfNode[]?     Nodes = null, TableOptions?    Options = null);
/// <summary>Input to <see cref="LpdfKit.Thead"/>.</summary>
public sealed record TheadInput   (LpdfNode[]?     Nodes = null, TheadOptions?    Options = null);
/// <summary>Input to <see cref="LpdfKit.Tr"/>.</summary>
public sealed record TrInput      (LpdfNode[]?     Nodes = null, TrOptions?       Options = null);
/// <summary>Input to <see cref="LpdfKit.Td"/>.</summary>
public sealed record TdInput      (LpdfNode[]?     Nodes = null, TdOptions?       Options = null);
/// <summary>Input to <see cref="LpdfKit.Page"/>.</summary>
public sealed record PageInput    (LpdfNode[]?     Nodes = null, PageOptions?     Options = null);
/// <summary>Input to <see cref="LpdfKit.Document"/>.</summary>
public sealed record DocumentInput(LpdfPageNode[]? Nodes = null, DocumentOptions? Options = null);

// ──────────────────────────────────────────────────────────────────────────────
// Options records — per-primitive, only valid attributes
// ──────────────────────────────────────────────────────────────────────────────

/// <summary>Attributes for the <c>stack</c> layout primitive.</summary>
public sealed record StackOptions(
    string? Gap        = null, string? Padding = null, string? Background = null,
    string? Align      = null, string? Justify = null, string? Width      = null,
    string? Height     = null, string? Border  = null, string? Radius     = null,
    string? Debug      = null);

/// <summary>Attributes for the <c>flank</c> layout primitive.</summary>
public sealed record FlankOptions(
    string? Gap        = null, string? Padding = null, string? Background = null,
    string? Align      = null, string? Justify = null, string? End        = null,
    string? Width      = null, string? Height  = null, string? Border     = null,
    string? Radius     = null, string? Debug   = null);

/// <summary>Attributes for the <c>split</c> layout primitive.</summary>
public sealed record SplitOptions(
    string? Gap        = null, string? Padding = null, string? Background = null,
    string? Align      = null, string? Equal   = null, string? Width      = null,
    string? Height     = null, string? Border  = null, string? Radius     = null,
    string? Debug      = null);

/// <summary>Attributes for the <c>cluster</c> layout primitive.</summary>
public sealed record ClusterOptions(
    string? Gap        = null, string? Padding = null, string? Background = null,
    string? Align      = null, string? Justify = null, string? Width      = null,
    string? Height     = null, string? Border  = null, string? Radius     = null,
    string? Debug      = null);

/// <summary>Attributes for the <c>grid</c> layout primitive.</summary>
public sealed record GridOptions(
    string? Cols       = null, string? ColWidth = null, string? Gap        = null,
    string? Equal      = null, string? Padding  = null, string? Background = null,
    string? Width      = null, string? Height   = null, string? Border     = null,
    string? Radius     = null, string? Debug    = null);

/// <summary>Attributes for the <c>frame</c> layout primitive.</summary>
public sealed record FrameOptions(
    string? Width      = null, string? Height     = null, string? Padding    = null,
    string? Background = null, string? Border     = null, string? Radius     = null,
    string? Align      = null, string? Debug      = null);

/// <summary>Attributes for the <c>link</c> layout primitive.</summary>
public sealed record LinkOptions(
    string? Url        = null, string? Width = null, string? Height = null);

/// <summary>Attributes for the <c>text</c> layout primitive.</summary>
public sealed record TextOptions(
    string? Font       = null, string? FontSize  = null, string? TextAlign = null,
    string? Color      = null, string? Bold      = null, string? End       = null,
    string? Repeat     = null, string? Width     = null, string? Height    = null,
    string? Padding    = null, string? Background = null, string? Border   = null,
    string? Radius     = null);

/// <summary>Attributes for the <c>span</c> inline primitive.</summary>
public sealed record SpanOptions(
    string? Font       = null, string? FontSize  = null, string? Color     = null,
    string? Bold       = null, string? Url       = null, string? Underline = null,
    string? Strike     = null);

/// <summary>Attributes for the <c>divider</c> primitive.</summary>
public sealed record DividerOptions(
    string? Color      = null, string? Thickness = null, string? Direction = null);

/// <summary>Attributes for the <c>img</c> primitive.</summary>
public sealed record ImgOptions(
    string  Name,
    string? Height     = null, string? Width      = null,
    string? Font       = null, string? FontSize   = null,
    string? Gap        = null, string? Padding    = null,
    string? Background = null, string? Border     = null,
    string? Radius     = null, string? Repeat     = null,
    string? Debug      = null);

/// <summary>Attributes for the <c>barcode</c> primitive.</summary>
public sealed record BarcodeOptions(
    string  Type,              string  Data,
    string? Size       = null, string? Width      = null,
    string? Height     = null, string? Ec         = null,
    string? Hrt        = null, string? Color      = null,
    string? Background = null, string? Repeat     = null,
    string? Debug      = null);

/// <summary>Attributes for the <c>table</c> layout primitive.</summary>
public sealed record TableOptions(
    string? Cols       = null, string? Border    = null, string? Stripe     = null,
    string? Gap        = null, string? Padding   = null, string? Background = null,
    string? Width      = null, string? Height    = null, string? Repeat     = null,
    string? Debug      = null);

/// <summary>Attributes for the <c>thead</c> layout primitive.</summary>
public sealed record TheadOptions(
    string? Background = null);

/// <summary>Attributes for the <c>tr</c> layout primitive.</summary>
public sealed record TrOptions(
    string? Background = null);

/// <summary>Attributes for the <c>td</c> layout primitive.</summary>
public sealed record TdOptions(
    string? Padding    = null, string? Background = null,
    string? Align      = null, string? Valign     = null,
    string? Border     = null, string? Radius     = null,
    string? Gap        = null, string? Debug      = null);

/// <summary>Attributes for the <c>page</c> primitive.</summary>
public sealed record PageOptions(
    string? Size       = null, string? Orientation = null, string? Margin   = null,
    string? Background = null, string? Debug       = null);

/// <summary>Document-level attributes applied as defaults to every page.</summary>
public sealed record DocumentOptions(
    string?      Size        = null, string?     Orientation = null,
    string?      Margin      = null, string?     Background  = null,
    LpdfTokens?  Tokens      = null, LpdfMeta?   Meta        = null);

// ──────────────────────────────────────────────────────────────────────────────
// Token + meta records
// ──────────────────────────────────────────────────────────────────────────────

/// <summary>Design-token overrides applied to the whole document.</summary>
public sealed record LpdfTokens(
    Dictionary<string, string>?   Colors = null,
    Dictionary<string, string>?   Space  = null,
    Dictionary<string, string>?   Grid   = null,
    Dictionary<string, string>?   Border = null,
    Dictionary<string, string>?   Radius = null,
    Dictionary<string, string>?   Width  = null,
    Dictionary<string, string>?   Text   = null,
    Dictionary<string, LpdfFont>? Fonts  = null);

/// <summary>Abstract base for a font definition used in <see cref="LpdfTokens"/>.</summary>
[JsonConverter(typeof(LpdfFontConverter))]
public abstract record LpdfFont;
/// <summary>Font loaded from a file path or URL supplied via <see cref="RenderOptions.SrcFallback"/>.</summary>
public sealed record LpdfFontSrc(string Src)         : LpdfFont;
/// <summary>One of the lpdf built-in fonts, referenced by name.</summary>
public sealed record LpdfFontBuiltin(string Builtin) : LpdfFont;

/// <summary>PDF document metadata written into the output file.</summary>
public sealed record LpdfMeta(
    string? Title    = null, string? Author   = null, string? Subject  = null,
    string? Keywords = null, string? Creator  = null);

// ──────────────────────────────────────────────────────────────────────────────
// Output node types — what LpdfKit helpers return / the serialised tree
// ──────────────────────────────────────────────────────────────────────────────

/// <summary>
/// Discriminated union of all layout nodes. Use <see cref="LpdfKit"/> to construct.
/// </summary>
[JsonConverter(typeof(LpdfNodeConverter))]
public abstract record LpdfNode
{
    /// <summary>The lpdf element name (e.g. <c>stack</c>, <c>text</c>, <c>page</c>).</summary>
    public abstract string Type { get; }
}

/// <summary>A text content item — either a raw string wrapped in <see cref="LpdfRawText"/> or an <see cref="LpdfSpanNode"/>.</summary>
public interface LpdfContent { }

/// <summary>A plain-text run inside a <c>text</c> node. Implicitly convertible from <see cref="string"/>.</summary>
public sealed record LpdfRawText(string Value) : LpdfContent
{
    /// <summary>Implicitly wrap a plain string as <see cref="LpdfRawText"/>.</summary>
    public static implicit operator LpdfRawText(string s) => new(s);
}

/// <summary>A serialised layout container node (stack, flank, grid, etc.).</summary>
public sealed record LpdfContainerNode(
    Dictionary<string, string> Attrs,
    List<LpdfNode>          Children,
    string                  _type) : LpdfNode
{
    public override string Type => _type;
}

/// <summary>A serialised <c>page</c> node.</summary>
public sealed record LpdfPageNode(
    Dictionary<string, string> Attrs,
    List<LpdfNode>          Children) : LpdfNode
{
    public override string Type => "page";
}

/// <summary>A serialised <c>text</c> paragraph node.</summary>
public sealed record LpdfTextNode(
    Dictionary<string, string>  Attrs,
    List<LpdfContent>           Children) : LpdfNode
{
    public override string Type => "text";
}

/// <summary>A serialised <c>span</c> inline node.</summary>
public sealed record LpdfSpanNode(
    Dictionary<string, string> Attrs,
    List<string>               Children) : LpdfNode, LpdfContent
{
    public override string Type => "span";
}

/// <summary>A serialised <c>divider</c> horizontal rule node.</summary>
public sealed record LpdfDividerNode(
    Dictionary<string, string> Attrs) : LpdfNode
{
    public override string Type => "divider";
}

/// <summary>A serialised <c>img</c> image node.</summary>
public sealed record LpdfImgNode(
    Dictionary<string, string> Attrs) : LpdfNode
{
    public override string Type => "img";
}

/// <summary>A serialised <c>barcode</c> node.</summary>
public sealed record LpdfBarcodeNode(
    Dictionary<string, string> Attrs) : LpdfNode
{
    public override string Type => "barcode";
}

/// <summary>Root document node — passed to <see cref="LpdfEngine.RenderPdf(LpdfDocument, RenderOptions?)"/>.</summary>
public sealed record LpdfDocument(
    [property: JsonPropertyName("attrs")]    Dictionary<string, object?> Attrs,
    [property: JsonPropertyName("children")] List<LpdfPageNode>          Children)
{
    /// <summary>Schema version; always <c>1</c>.</summary>
    [JsonPropertyName("version")] public int    Version { get; } = 1;
    /// <summary>Node type discriminator; always <c>"document"</c>.</summary>
    [JsonPropertyName("type")]    public string Type    { get; } = "document";
}

// ──────────────────────────────────────────────────────────────────────────────
// JSON converters
// ──────────────────────────────────────────────────────────────────────────────

internal sealed class LpdfNodeConverter : JsonConverter<LpdfNode>
{
    public override bool CanConvert(Type typeToConvert)
        => typeof(LpdfNode).IsAssignableFrom(typeToConvert);

    public override LpdfNode Read(ref Utf8JsonReader _, Type __, JsonSerializerOptions ___)
        => throw new NotSupportedException();

    public override void Write(Utf8JsonWriter writer, LpdfNode value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        writer.WriteString("type", value.Type);

        switch (value)
        {
            case LpdfContainerNode c:
                WriteAttrs(writer, c.Attrs);
                WriteChildren(writer, c.Children, options);
                break;
            case LpdfPageNode p:
                WriteAttrs(writer, p.Attrs);
                WriteChildren(writer, p.Children, options);
                break;
            case LpdfTextNode t:
                WriteAttrs(writer, t.Attrs);
                WriteTextChildren(writer, t.Children, options);
                break;
            case LpdfSpanNode s:
                WriteAttrs(writer, s.Attrs);
                writer.WriteStartArray("children");
                foreach (var str in s.Children) writer.WriteStringValue(str);
                writer.WriteEndArray();
                break;
            case LpdfDividerNode d:
                WriteAttrs(writer, d.Attrs);
                break;
            case LpdfImgNode img:
                WriteAttrs(writer, img.Attrs);
                break;
            case LpdfBarcodeNode bc:
                WriteAttrs(writer, bc.Attrs);
                break;
        }

        writer.WriteEndObject();
    }

    private static void WriteAttrs(Utf8JsonWriter writer, Dictionary<string, string> attrs)
    {
        writer.WriteStartObject("attrs");
        foreach (var (k, v) in attrs) writer.WriteString(k, v);
        writer.WriteEndObject();
    }

    private static void WriteChildren(Utf8JsonWriter writer, List<LpdfNode> children, JsonSerializerOptions options)
    {
        writer.WriteStartArray("children");
        foreach (var child in children)
            JsonSerializer.Serialize(writer, child, options);
        writer.WriteEndArray();
    }

    private static void WriteTextChildren(Utf8JsonWriter writer, List<LpdfContent> children, JsonSerializerOptions options)
    {
        writer.WriteStartArray("children");
        foreach (var item in children)
        {
            if (item is LpdfRawText raw)
                writer.WriteStringValue(raw.Value);
            else if (item is LpdfSpanNode span)
                JsonSerializer.Serialize(writer, (LpdfNode)span, options);
        }
        writer.WriteEndArray();
    }
}

internal sealed class LpdfFontConverter : JsonConverter<LpdfFont>
{
    public override LpdfFont Read(ref Utf8JsonReader _, Type __, JsonSerializerOptions ___)
        => throw new NotSupportedException();

    public override void Write(Utf8JsonWriter writer, LpdfFont value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        if (value is LpdfFontSrc src)        writer.WriteString("src",     src.Src);
        if (value is LpdfFontBuiltin builtin) writer.WriteString("builtin", builtin.Builtin);
        writer.WriteEndObject();
    }
}

internal static class LpdfDocumentJson
{
    /// <summary>Options used when serializing an <see cref="LpdfDocument"/> to the WASM input JSON.</summary>
    internal static readonly JsonSerializerOptions Options = new()
    {
        // Property names on LpdfDocument carry explicit [JsonPropertyName] attributes;
        // everything else (Attrs dict keys, anonymous objects in tokens) is
        // already lower-case by construction.
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        Converters = { new LpdfNodeConverter(), new LpdfFontConverter() },
    };
}
