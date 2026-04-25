namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>A <c>canvas-line</c> node — a straight line segment.</summary>
public sealed record LineNode(
    double     X1, double Y1, double X2, double Y2,
    LineStyle? Style = null) : CanvasNode
{
    /// <inheritdoc/>
    public override string Type => "canvas-line";
}
