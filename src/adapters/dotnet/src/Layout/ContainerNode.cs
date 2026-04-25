namespace Lpdf.Layout;

/// <summary>A serialised layout container node (stack, flank, grid, table, etc.).</summary>
public sealed record ContainerNode(
    Dictionary<string, string> Attrs,
    List<Node>                 Nodes,
    string                     _type) : Node
{
    /// <inheritdoc/>
    public override string Type => _type;
}
