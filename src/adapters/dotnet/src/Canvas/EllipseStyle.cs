namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>Style for <c>canvas-ellipse</c> and <c>canvas-circle</c> nodes.</summary>
public sealed record EllipseStyle(
    string?  Fill        = null,
    string?  Stroke      = null,
    double?  StrokeWidth = null,
    double?  StrokeDash  = null);
