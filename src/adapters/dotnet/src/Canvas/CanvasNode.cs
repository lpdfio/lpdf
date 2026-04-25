using System.Text.Json.Serialization;

namespace Lpdf.Canvas;

#pragma warning disable CS1591

/// <summary>
/// Abstract base for all canvas node types.
/// Use <see cref="LpdfCanvas"/> to construct instances.
/// </summary>
[JsonConverter(typeof(Lpdf.Shared.CanvasNodeConverter))]
public abstract record CanvasNode
{
    /// <summary>The canvas element name (e.g. <c>canvas-rect</c>, <c>canvas-text</c>).</summary>
    public abstract string Type { get; }
}
