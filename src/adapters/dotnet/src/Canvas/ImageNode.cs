namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>A <c>canvas-image</c> node — a raster image at an absolute position.</summary>
public sealed record ImageNode(
    double  X, double Y,
    string  Name,
    double? W = null, double? H = null) : CanvasNode
{
    /// <inheritdoc/>
    public override string Type => "canvas-image";
}
