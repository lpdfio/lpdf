using Lpdf.Layout;

namespace Lpdf.Kit;

/// <summary>
/// A layout block inside a <see cref="SectionNode"/>, containing layout nodes.
/// Produced by <see cref="LpdfKit.Layout"/>.
/// </summary>
public sealed record SectionLayout(
    List<Node>                 Nodes,
    Dictionary<string, string> Attrs) : SectionContent
{
    /// <inheritdoc/>
    public override string Type => "layout";
}
