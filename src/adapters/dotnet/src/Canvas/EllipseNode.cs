namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>A <c>canvas-ellipse</c> node.</summary>
public sealed record EllipseNode(
    double       Cx, double Cy, double Rx, double Ry,
    EllipseStyle? Style = null) : CanvasNode
{
    /// <inheritdoc/>
    public override string Type => "canvas-ellipse";
}
