namespace Lpdf.Layout;

/// <summary>A serialised <c>region</c> node — an absolutely positioned overlay/underlay block.</summary>
public sealed record RegionNode(
    Dictionary<string, string> Attrs,
    List<Node>                 Nodes) : Node
{
    /// <inheritdoc/>
    public override string Type => "region";
}
