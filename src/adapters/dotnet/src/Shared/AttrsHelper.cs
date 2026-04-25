using System.Collections.Concurrent;
using System.Reflection;

namespace Lpdf.Shared;

#pragma warning disable CS1591

internal static class AttrsHelper
{
    private static readonly ConcurrentDictionary<Type, PropertyInfo[]> _propCache = new();

    /// <summary>
    /// Reflect over any options record and convert its non-null string properties to
    /// a kebab-case attribute dictionary.
    /// </summary>
    internal static Dictionary<string, string> Attrs(object? options)
    {
        var result = new Dictionary<string, string>(StringComparer.Ordinal);
        if (options is null) return result;

        var props = _propCache.GetOrAdd(
            options.GetType(),
            t => t.GetProperties(BindingFlags.Public | BindingFlags.Instance));
        foreach (var prop in props)
        {
            var value = prop.GetValue(options);
            if (value is string s)
                result[PascalToKebab(prop.Name)] = s;
        }
        return result;
    }

    /// <summary>PascalCase → kebab-case (e.g. <c>FontSize</c> → <c>font-size</c>).</summary>
    internal static string PascalToKebab(string name)
    {
        var sb = new System.Text.StringBuilder(name.Length + 4);
        for (var i = 0; i < name.Length; i++)
        {
            var c = name[i];
            if (char.IsUpper(c) && i > 0) sb.Append('-');
            sb.Append(char.ToLowerInvariant(c));
        }
        return sb.ToString();
    }
}
