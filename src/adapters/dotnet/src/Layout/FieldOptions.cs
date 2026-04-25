namespace Lpdf.Layout;

#pragma warning disable CS1591
/// <summary>Attributes for the <c>field</c> form primitive.</summary>
public sealed record FieldOptions(
    string  Type,
    string  Name,
    string? Value      = null, string? Width   = null, string? Height = null,
    string? Font       = null, string? FontSize = null, string? Color = null,
    string? Background = null, string? Border  = null, string? Radius = null,
    string? Required   = null, string? Readonly = null, string? Debug = null);
