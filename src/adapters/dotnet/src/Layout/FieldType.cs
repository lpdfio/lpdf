namespace Lpdf.Layout;

/// <summary>Form field type discriminator.</summary>
public enum FieldType
{
    /// <summary>Single-line text input.</summary>
    Text,
    /// <summary>Multi-line text area.</summary>
    Multiline,
    /// <summary>Checkbox.</summary>
    Checkbox,
    /// <summary>Radio button.</summary>
    Radio,
    /// <summary>Signature field.</summary>
    Signature,
}
