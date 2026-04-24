<?php

declare(strict_types=1);

namespace Lpdf\Shared;

/** @internal Shared by Kit and Layout. */
trait AttrsHelper
{
    /**
     * Reflect over a readonly options object and convert its non-null
     * string, bool, or BackedEnum properties to a kebab-case attribute map,
     * skipping named properties.
     *
     * Supported value types and their serialisation:
     *   - string      → emitted as-is
     *   - bool        → emitted as 'true' or 'false'
     *   - BackedEnum  → emitted as the backing value string
     *
     * Any other type (int, float, array, …) is silently skipped.  Options
     * classes should use only the types above to avoid silent data loss.
     *
     * @param  string[] $skip Property names to exclude (handled separately).
     * @return array<string,string>
     */
    private static function optionsToAttrs(?object $options, array $skip = []): array
    {
        if ($options === null) {
            return [];
        }

        $attrs = [];
        foreach ((new \ReflectionClass($options))->getProperties() as $prop) {
            $name = $prop->getName();
            if (in_array($name, $skip, true)) {
                continue;
            }
            $value = $prop->getValue($options);
            if ($value instanceof \BackedEnum) {
                $attrs[self::camelToKebab($name)] = $value->value;
            } elseif (is_bool($value)) {
                $attrs[self::camelToKebab($name)] = $value ? 'true' : 'false';
            } elseif (is_string($value)) {
                $attrs[self::camelToKebab($name)] = $value;
            }
        }

        return $attrs;
    }

    /** camelCase / PascalCase → kebab-case. */
    private static function camelToKebab(string $name): string
    {
        return strtolower((string) preg_replace('/[A-Z]/', '-$0', lcfirst($name)));
    }
}
