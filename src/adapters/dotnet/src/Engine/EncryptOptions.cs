namespace Lpdf.Engine;

/// <summary>
/// RC4-128 encryption options passed to <see cref="LpdfEngine.SetEncryption"/>.
/// </summary>
public sealed class EncryptOptions
{
    /// <summary>Open password shown to readers. Empty string = no open password required.</summary>
    public string UserPassword  { get; init; } = "";
    /// <summary>Owner (permissions) password. Required; must be non-empty.</summary>
    public string OwnerPassword { get; init; } = "";
    /// <summary>Permission flags applied to the document.</summary>
    public EncryptPermissions Permissions { get; init; } = new();
}
