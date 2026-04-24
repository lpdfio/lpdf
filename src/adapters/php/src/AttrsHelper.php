<?php

declare(strict_types=1);

namespace Lpdf;

/** @internal Shared by Kit and Layout. */
trait AttrsHelper
{
    /**
     * Reflect over a readonly options object and convert its non-null string
     * properties to a kebab-case attribute map, skipping named properties.
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
            if (!is_string($value)) {
                continue;
            }
            $attrs[self::camelToKebab($name)] = $value;
        }

        return $attrs;
    }

    /** camelCase / PascalCase → kebab-case. */
    private static function camelToKebab(string $name): string
    {
        return strtolower((string) preg_replace('/[A-Z]/', '-$0', lcfirst($name)));
    }
}
