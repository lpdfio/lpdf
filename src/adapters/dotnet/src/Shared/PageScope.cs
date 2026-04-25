namespace Lpdf.Shared;

/// <summary>
/// Well-known <c>page</c> scope values for <see cref="Lpdf.Canvas.LayerOptions.Page"/>
/// and <see cref="Lpdf.Layout.RegionOptions.Page"/>.
/// Raw range strings such as <c>"2-4"</c>, <c>"1,3-5"</c>, or <c>"2-last"</c>
/// may be passed directly without using these constants.
/// </summary>
public static class PageScope
{
    /// <summary>Apply to every page.</summary>
    public const string Each  = "each";
    /// <summary>Apply to the first page only.</summary>
    public const string First = "first";
    /// <summary>Apply to the last page only.</summary>
    public const string Last  = "last";
    /// <summary>Apply to odd-numbered pages.</summary>
    public const string Odd   = "odd";
    /// <summary>Apply to even-numbered pages.</summary>
    public const string Even  = "even";
}
