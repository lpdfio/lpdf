namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>Style for <c>canvas-rect</c> nodes.</summary>
public sealed record RectStyle(
    string?  Fill         = null,
    string?  Stroke       = null,
    double?  StrokeWidth  = null,
    double?  StrokeDash   = null,
    double?  BorderRadius = null);
