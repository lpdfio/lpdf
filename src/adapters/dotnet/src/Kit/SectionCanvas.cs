using Lpdf.Canvas;

namespace Lpdf.Kit;

/// <summary>
/// A canvas block inside a <see cref="SectionNode"/>, containing canvas layer nodes.
/// Produced by <see cref="LpdfKit.Canvas"/>.
/// </summary>
public sealed record SectionCanvas(
    List<LayerNode>            Layers,
    Dictionary<string, string> Attrs) : SectionContent
{
    /// <inheritdoc/>
    public override string Type => "canvas";
}
