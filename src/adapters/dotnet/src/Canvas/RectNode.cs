namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>A <c>canvas-rect</c> node — an axis-aligned rectangle.</summary>
public sealed record RectNode(
    double     X, double Y, double W, double H,
    RectStyle? Style = null) : CanvasNode
{
    /// <inheritdoc/>
    public override string Type => "canvas-rect";
}
