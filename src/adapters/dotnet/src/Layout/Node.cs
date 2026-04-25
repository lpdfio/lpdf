using System.Text.Json.Serialization;

namespace Lpdf.Layout;

#pragma warning disable CS1591

/// <summary>
/// Discriminated union of all layout nodes. Use <see cref="LpdfLayout"/> to construct.
/// </summary>
[JsonConverter(typeof(Lpdf.Shared.NodeConverter))]
public abstract record Node
{
    /// <summary>The lpdf element name (e.g. <c>stack</c>, <c>text</c>, <c>img</c>).</summary>
    public abstract string Type { get; }
}
