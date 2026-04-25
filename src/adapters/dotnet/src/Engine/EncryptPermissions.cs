namespace Lpdf.Engine;

/// <summary>
/// PDF permission flags for RC4-128 encryption.
/// All flags default to <c>true</c> (allowed); set to <c>false</c> to restrict.
/// </summary>
public sealed class EncryptPermissions
{
    /// <summary>Allow printing.</summary>
    public bool Print         { get; init; } = true;
    /// <summary>Allow content modification.</summary>
    public bool Modify        { get; init; } = true;
    /// <summary>Allow text and graphic extraction.</summary>
    public bool Copy          { get; init; } = true;
    /// <summary>Allow adding or modifying annotations.</summary>
    public bool Annotate      { get; init; } = true;
    /// <summary>Allow form field filling.</summary>
    public bool FillForms     { get; init; } = true;
    /// <summary>Allow accessibility tools (screen readers).</summary>
    public bool Accessibility { get; init; } = true;
    /// <summary>Allow page insertion, deletion, and rotation.</summary>
    public bool Assemble      { get; init; } = true;
    /// <summary>Allow high-quality printing.</summary>
    public bool PrintHq       { get; init; } = true;
}
