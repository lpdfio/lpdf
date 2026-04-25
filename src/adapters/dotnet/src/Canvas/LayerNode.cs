namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>A <c>canvas-layer</c> node — a rendering layer containing canvas primitives.</summary>
public sealed record LayerNode(
    List<CanvasNode> Nodes,
    LayerOptions?    Options = null) : CanvasNode
{
    /// <inheritdoc/>
    public override string Type => "canvas-layer";
}
