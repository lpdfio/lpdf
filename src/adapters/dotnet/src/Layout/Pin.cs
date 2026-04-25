namespace Lpdf.Layout;

#pragma warning disable CS1591
/// <summary>
/// Pin positioning helper — an absolute-position anchor for a node within its container.
/// Pass as the <c>pin</c> attribute value using <see cref="ToString"/>.
/// </summary>
public sealed record Pin(string? Top = null, string? Right = null, string? Bottom = null, string? Left = null)
{
    /// <summary>Renders the pin as a CSS shorthand string, e.g. <c>"top:10pt right:20pt"</c>.</summary>
    public override string ToString()
    {
        var parts = new List<string>(4);
        if (Top    is not null) parts.Add($"top:{Top}");
        if (Right  is not null) parts.Add($"right:{Right}");
        if (Bottom is not null) parts.Add($"bottom:{Bottom}");
        if (Left   is not null) parts.Add($"left:{Left}");
        return string.Join(" ", parts);
    }
}
