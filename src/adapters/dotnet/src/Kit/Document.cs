using System.Text.Json.Serialization;

namespace Lpdf.Kit;

/// <summary>Root document node — passed to <see cref="LpdfEngine.RenderPdf(Document, Lpdf.Engine.RenderOptions?)"/>.</summary>
public sealed record Document(
    [property: JsonPropertyName("attrs")] Dictionary<string, object?> Attrs,
    [property: JsonPropertyName("nodes")] List<SectionNode>           Nodes)
{
    /// <summary>Schema version; always <c>1</c>.</summary>
    [JsonPropertyName("version")] public int    Version { get; } = 1;
    /// <summary>Node type discriminator; always <c>"document"</c>.</summary>
    [JsonPropertyName("type")]    public string Type    { get; } = "document";
}
