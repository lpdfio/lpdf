namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>Rectangular clip region applied to a canvas layer.</summary>
public sealed record Clip(double X, double Y, double W, double H, double? BorderRadius = null);
