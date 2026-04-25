using System.Text.Json;
using System.Text.Json.Serialization;
using Lpdf.Shared;

namespace Lpdf.Kit;

/// <summary>Abstract base for a font definition used in <see cref="DocumentTokens"/>.</summary>
[JsonConverter(typeof(FontConverter))]
public abstract record Font;

internal sealed class FontConverter : JsonConverter<Font>
{
    public override Font Read(ref Utf8JsonReader _, Type __, JsonSerializerOptions ___)
        => throw new NotSupportedException();

    public override void Write(Utf8JsonWriter writer, Font value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        if (value is FontSrc src)        writer.WriteString("src",     src.Src);
        if (value is FontBuiltin builtin) writer.WriteString("builtin", builtin.Builtin);
        writer.WriteEndObject();
    }
}
