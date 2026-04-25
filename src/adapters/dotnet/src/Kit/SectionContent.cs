using Lpdf.Layout;

namespace Lpdf.Kit;

/// <summary>
/// Abstract base for content that can appear directly inside a <see cref="SectionNode"/>.
/// Use <see cref="LpdfKit.Layout(Node[])"/> or <see cref="LpdfKit.Canvas(Lpdf.Canvas.LayerNode[])"/>
/// to create instances.
/// </summary>
public abstract record SectionContent
{
    /// <summary>The element type name emitted to the wire format.</summary>
    public abstract string Type { get; }
}
