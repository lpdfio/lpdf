namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>A <c>canvas-circle</c> node (uniform radii convenience form).</summary>
public sealed record CircleNode(
    double        Cx, double Cy, double R,
    EllipseStyle? Style = null) : CanvasNode
{
    /// <inheritdoc/>
    public override string Type => "canvas-circle";
}
