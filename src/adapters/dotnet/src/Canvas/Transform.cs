namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>A 6-element affine transform matrix <c>[a, b, c, d, e, f]</c>.</summary>
public sealed record Transform(double A, double B, double C, double D, double E, double F);
