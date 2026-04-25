using System.Text.Json;
using System.Text.Json.Serialization;
using Lpdf.Canvas;
using Lpdf.Kit;
using Lpdf.Layout;

namespace Lpdf.Shared;

// ──────────────────────────────────────────────────────────────────────────────
// NodeConverter — serialises Lpdf.Layout.Node subtypes
// ──────────────────────────────────────────────────────────────────────────────

internal sealed class NodeConverter : JsonConverter<Layout.Node>
{
    public override bool CanConvert(Type typeToConvert)
        => typeof(Layout.Node).IsAssignableFrom(typeToConvert);

    public override Layout.Node Read(ref Utf8JsonReader _, Type __, JsonSerializerOptions ___)
        => throw new NotSupportedException();

    public override void Write(Utf8JsonWriter writer, Layout.Node value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        writer.WriteString("type", value.Type);

        switch (value)
        {
            case ContainerNode c:
                WriteAttrs(writer, c.Attrs);
                WriteLayoutNodes(writer, c.Nodes, options);
                break;
            case Layout.TextNode t:
                WriteAttrs(writer, t.Attrs);
                WriteTextContent(writer, t.Children, options);
                break;
            case SpanNode s:
                WriteAttrs(writer, s.Attrs);
                writer.WriteStartArray("nodes");
                foreach (var str in s.Children) writer.WriteStringValue(str);
                writer.WriteEndArray();
                break;
            case RegionNode r:
                WriteAttrs(writer, r.Attrs);
                WriteLayoutNodes(writer, r.Nodes, options);
                break;
            case DividerNode d:
                WriteAttrs(writer, d.Attrs);
                break;
            case ImgNode img:
                WriteAttrs(writer, img.Attrs);
                break;
            case BarcodeNode bc:
                WriteAttrs(writer, bc.Attrs);
                break;
            case FieldNode f:
                WriteAttrs(writer, f.Attrs);
                break;
        }

        writer.WriteEndObject();
    }

    internal static void WriteAttrs(Utf8JsonWriter writer, Dictionary<string, string> attrs)
    {
        writer.WriteStartObject("attrs");
        foreach (var (k, v) in attrs) writer.WriteString(k, v);
        writer.WriteEndObject();
    }

    internal static void WriteLayoutNodes(Utf8JsonWriter writer, List<Layout.Node> nodes, JsonSerializerOptions options)
    {
        writer.WriteStartArray("nodes");
        foreach (var child in nodes)
            JsonSerializer.Serialize(writer, child, options);
        writer.WriteEndArray();
    }

    private static void WriteTextContent(Utf8JsonWriter writer, List<Content> children, JsonSerializerOptions options)
    {
        writer.WriteStartArray("nodes");
        foreach (var item in children)
        {
            if (item is RawText raw)
                writer.WriteStringValue(raw.Value);
            else if (item is SpanNode span)
                JsonSerializer.Serialize(writer, (Layout.Node)span, options);
        }
        writer.WriteEndArray();
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// CanvasNodeConverter — serialises Lpdf.Canvas.CanvasNode subtypes
// ──────────────────────────────────────────────────────────────────────────────

internal sealed class CanvasNodeConverter : JsonConverter<CanvasNode>
{
    public override bool CanConvert(Type typeToConvert)
        => typeof(CanvasNode).IsAssignableFrom(typeToConvert);

    public override CanvasNode Read(ref Utf8JsonReader _, Type __, JsonSerializerOptions ___)
        => throw new NotSupportedException();

    public override void Write(Utf8JsonWriter writer, CanvasNode value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        writer.WriteString("type", value.Type);

        switch (value)
        {
            case LayerNode layer:
                WriteLayerOptions(writer, layer.Options);
                writer.WriteStartArray("nodes");
                foreach (var node in layer.Nodes)
                    JsonSerializer.Serialize(writer, node, options);
                writer.WriteEndArray();
                break;
            case RectNode rect:
                writer.WriteNumber("x", rect.X);
                writer.WriteNumber("y", rect.Y);
                writer.WriteNumber("w", rect.W);
                writer.WriteNumber("h", rect.H);
                WriteRectStyle(writer, rect.Style);
                break;
            case LineNode line:
                writer.WriteNumber("x1", line.X1);
                writer.WriteNumber("y1", line.Y1);
                writer.WriteNumber("x2", line.X2);
                writer.WriteNumber("y2", line.Y2);
                WriteLineStyle(writer, line.Style);
                break;
            case EllipseNode ellipse:
                writer.WriteNumber("cx", ellipse.Cx);
                writer.WriteNumber("cy", ellipse.Cy);
                writer.WriteNumber("rx", ellipse.Rx);
                writer.WriteNumber("ry", ellipse.Ry);
                WriteEllipseStyle(writer, ellipse.Style);
                break;
            case CircleNode circle:
                writer.WriteNumber("cx", circle.Cx);
                writer.WriteNumber("cy", circle.Cy);
                writer.WriteNumber("r",  circle.R);
                WriteEllipseStyle(writer, circle.Style);
                break;
            case PathNode path:
                writer.WriteString("d", path.D);
                WritePathStyle(writer, path.Style);
                break;
            case Canvas.TextNode text:
                writer.WriteNumber("x", text.X);
                writer.WriteNumber("y", text.Y);
                writer.WriteString("content", text.Content);
                WriteTextStyle(writer, text.Style);
                WriteRuns(writer, text.Runs, options);
                break;
            case ImageNode img:
                writer.WriteNumber("x", img.X);
                writer.WriteNumber("y", img.Y);
                writer.WriteString("name", img.Name);
                if (img.W.HasValue) writer.WriteNumber("w", img.W.Value);
                if (img.H.HasValue) writer.WriteNumber("h", img.H.Value);
                break;
        }

        writer.WriteEndObject();
    }

    private static void WriteLayerOptions(Utf8JsonWriter writer, LayerOptions? opts)
    {
        if (opts is null) return;
        if (opts.Page    is not null) writer.WriteString("page",    opts.Page);
        if (opts.Opacity.HasValue)    writer.WriteNumber("opacity", opts.Opacity.Value);
        if (opts.Transform is { } t)
        {
            writer.WriteStartArray("transform");
            writer.WriteNumberValue(t.A); writer.WriteNumberValue(t.B);
            writer.WriteNumberValue(t.C); writer.WriteNumberValue(t.D);
            writer.WriteNumberValue(t.E); writer.WriteNumberValue(t.F);
            writer.WriteEndArray();
        }
        if (opts.Clip is { } c)
        {
            writer.WriteStartObject("clip");
            writer.WriteNumber("x", c.X); writer.WriteNumber("y", c.Y);
            writer.WriteNumber("w", c.W); writer.WriteNumber("h", c.H);
            if (c.BorderRadius.HasValue) writer.WriteNumber("borderRadius", c.BorderRadius.Value);
            writer.WriteEndObject();
        }
    }

    private static void WriteRectStyle(Utf8JsonWriter writer, RectStyle? s)
    {
        if (s is null) return;
        if (s.Fill         is not null) writer.WriteString("fill",         s.Fill);
        if (s.Stroke       is not null) writer.WriteString("stroke",       s.Stroke);
        if (s.StrokeWidth.HasValue)     writer.WriteNumber("strokeWidth",  s.StrokeWidth.Value);
        if (s.StrokeDash.HasValue)      writer.WriteNumber("strokeDash",   s.StrokeDash.Value);
        if (s.BorderRadius.HasValue)    writer.WriteNumber("borderRadius", s.BorderRadius.Value);
    }

    private static void WriteLineStyle(Utf8JsonWriter writer, LineStyle? s)
    {
        if (s is null) return;
        if (s.Stroke       is not null) writer.WriteString("stroke",      s.Stroke);
        if (s.StrokeWidth.HasValue)     writer.WriteNumber("strokeWidth", s.StrokeWidth.Value);
        if (s.StrokeDash.HasValue)      writer.WriteNumber("strokeDash",  s.StrokeDash.Value);
        if (s.LineCap.HasValue)         writer.WriteString("lineCap",     s.LineCap.Value.ToString().ToLowerInvariant());
        if (s.LineJoin.HasValue)        writer.WriteString("lineJoin",    s.LineJoin.Value.ToString().ToLowerInvariant());
    }

    private static void WriteEllipseStyle(Utf8JsonWriter writer, EllipseStyle? s)
    {
        if (s is null) return;
        if (s.Fill        is not null) writer.WriteString("fill",        s.Fill);
        if (s.Stroke      is not null) writer.WriteString("stroke",      s.Stroke);
        if (s.StrokeWidth.HasValue)    writer.WriteNumber("strokeWidth", s.StrokeWidth.Value);
        if (s.StrokeDash.HasValue)     writer.WriteNumber("strokeDash",  s.StrokeDash.Value);
    }

    private static void WritePathStyle(Utf8JsonWriter writer, PathStyle? s)
    {
        if (s is null) return;
        if (s.Fill        is not null) writer.WriteString("fill",        s.Fill);
        if (s.Stroke      is not null) writer.WriteString("stroke",      s.Stroke);
        if (s.StrokeWidth.HasValue)    writer.WriteNumber("strokeWidth", s.StrokeWidth.Value);
        if (s.StrokeDash.HasValue)     writer.WriteNumber("strokeDash",  s.StrokeDash.Value);
    }

    private static void WriteTextStyle(Utf8JsonWriter writer, Canvas.TextStyle? s)
    {
        if (s is null) return;
        if (s.Font       is not null) writer.WriteString("font",       s.Font);
        if (s.Size.HasValue)          writer.WriteNumber("size",       s.Size.Value);
        if (s.Color      is not null) writer.WriteString("color",      s.Color);
        if (s.Align.HasValue)         writer.WriteString("align",      s.Align.Value.ToString().ToLowerInvariant());
        if (s.LineHeight.HasValue)    writer.WriteNumber("lineHeight", s.LineHeight.Value);
        if (s.Width.HasValue)         writer.WriteNumber("width",      s.Width.Value);
    }

    private static void WriteRuns(Utf8JsonWriter writer, Run[]? runs, JsonSerializerOptions options)
    {
        if (runs is null || runs.Length == 0) return;
        writer.WriteStartArray("runs");
        foreach (var r in runs)
        {
            writer.WriteStartObject();
            writer.WriteString("text", r.Text);
            if (r.Font  is not null) writer.WriteString("font",  r.Font);
            if (r.Size.HasValue)     writer.WriteNumber("size",  r.Size.Value);
            if (r.Color is not null) writer.WriteString("color", r.Color);
            writer.WriteEndObject();
        }
        writer.WriteEndArray();
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// SectionContentConverter — serialises SectionLayout and SectionCanvas
// ──────────────────────────────────────────────────────────────────────────────

internal sealed class SectionContentConverter : JsonConverter<SectionContent>
{
    public override bool CanConvert(Type typeToConvert)
        => typeof(SectionContent).IsAssignableFrom(typeToConvert);

    public override SectionContent Read(ref Utf8JsonReader _, Type __, JsonSerializerOptions ___)
        => throw new NotSupportedException();

    public override void Write(Utf8JsonWriter writer, SectionContent value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        writer.WriteString("type", value.Type);
        writer.WriteStartObject("attrs");
        if (value is SectionLayout sl)
        {
            // Write extra attrs if any were set (currently attrs dict is empty by convention)
            foreach (var (k, v) in sl.Attrs) writer.WriteString(k, v);
        }
        else if (value is SectionCanvas sc)
        {
            foreach (var (k, v) in sc.Attrs) writer.WriteString(k, v);
        }
        writer.WriteEndObject();

        writer.WriteStartArray("nodes");
        if (value is SectionLayout layout)
        {
            foreach (var node in layout.Nodes)
                JsonSerializer.Serialize(writer, node, options);
        }
        else if (value is SectionCanvas canvas)
        {
            foreach (var layer in canvas.Layers)
                JsonSerializer.Serialize(writer, layer, options);
        }
        writer.WriteEndArray();

        writer.WriteEndObject();
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// SectionNodeConverter — serialises Kit.SectionNode
// ──────────────────────────────────────────────────────────────────────────────

internal sealed class SectionNodeConverter : JsonConverter<SectionNode>
{
    public override SectionNode Read(ref Utf8JsonReader _, Type __, JsonSerializerOptions ___)
        => throw new NotSupportedException();

    public override void Write(Utf8JsonWriter writer, SectionNode value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        writer.WriteString("type", value.Type);
        NodeConverter.WriteAttrs(writer, value.Attrs);

        writer.WriteStartArray("nodes");
        foreach (var content in value.Nodes)
            JsonSerializer.Serialize(writer, content, options);
        writer.WriteEndArray();

        writer.WriteEndObject();
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// FontConverter — serialises Kit.Font subtypes
// ──────────────────────────────────────────────────────────────────────────────

// Note: FontConverter is also defined on Kit.Font via [JsonConverter] attribute.
// This duplicate registration in DocumentJson.Options is harmless — it ensures
// the converter is used even if the attribute is not visible in the options chain.

// ──────────────────────────────────────────────────────────────────────────────
// DocumentJson — shared serialisation options for Document → WASM JSON
// ──────────────────────────────────────────────────────────────────────────────

internal static class DocumentJson
{
    internal static readonly JsonSerializerOptions Options = new()
    {
        DefaultIgnoreCondition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull,
        Converters =
        {
            new NodeConverter(),
            new CanvasNodeConverter(),
            new SectionContentConverter(),
            new SectionNodeConverter(),
            new Kit.FontConverter(),
        },
    };
}
