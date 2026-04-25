namespace Lpdf.Kit;

/// <summary>A serialised <c>section</c> (page) node.</summary>
public sealed record SectionNode(
    Dictionary<string, string> Attrs,
    List<SectionContent>       Nodes)
{
    /// <summary>The element type; always <c>"section"</c>.</summary>
    public string Type => "section";
}
