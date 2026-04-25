namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>A <c>canvas-path</c> node — an SVG-syntax path.</summary>
public sealed record PathNode(
    string     D,
    PathStyle? Style = null) : CanvasNode
{
    /// <inheritdoc/>
    public override string Type => "canvas-path";
}
