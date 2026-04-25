namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>Style for <c>canvas-line</c> nodes.</summary>
public sealed record LineStyle(
    string?   Stroke      = null,
    double?   StrokeWidth = null,
    double?   StrokeDash  = null,
    LineCap?  LineCap     = null,
    LineJoin? LineJoin    = null);
