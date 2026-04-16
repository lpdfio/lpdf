<?php

declare(strict_types=1);

namespace Lpdf;

/**
 * Static tree-builder helpers for constructing lpdf document trees programmatically.
 *
 * All helpers return plain serialisable objects. Pass the result of
 * {@see document()} to {@see LpdfEngine::renderPdf()}.
 *
 * @example
 * ```php
 * use Lpdf\{LpdfKit, LpdfEngine, PageOptions, DocumentOptions, LpdfMeta};
 *
 * $doc = LpdfKit::document(
 *     nodes: [
 *         LpdfKit::page(
 *             nodes: [LpdfKit::text(nodes: ['Hello, world!'])],
 *             options: new PageOptions(size: 'a4', margin: '28pt'),
 *         ),
 *     ],
 *     options: new DocumentOptions(meta: new LpdfMeta(title: 'My Doc')),
 * );
 * $pdf = (new LpdfEngine(''))->renderPdf($doc);
 * ```
 */
final class LpdfKit
{
    // ── Container helpers ─────────────────────────────────────────────────────

    /** @param LpdfNode[] $nodes */
    public static function stack(array $nodes = [], ?StackOptions $options = null): LpdfContainerNode
    {
        return new LpdfContainerNode('stack', self::optionsToAttrs($options), $nodes);
    }

    /** @param LpdfNode[] $nodes */
    public static function flank(array $nodes = [], ?FlankOptions $options = null): LpdfContainerNode
    {
        return new LpdfContainerNode('flank', self::optionsToAttrs($options), $nodes);
    }

    /** @param LpdfNode[] $nodes */
    public static function split(array $nodes = [], ?SplitOptions $options = null): LpdfContainerNode
    {
        return new LpdfContainerNode('split', self::optionsToAttrs($options), $nodes);
    }

    /** @param LpdfNode[] $nodes */
    public static function cluster(array $nodes = [], ?ClusterOptions $options = null): LpdfContainerNode
    {
        return new LpdfContainerNode('cluster', self::optionsToAttrs($options), $nodes);
    }

    /** @param LpdfNode[] $nodes */
    public static function grid(array $nodes = [], ?GridOptions $options = null): LpdfContainerNode
    {
        return new LpdfContainerNode('grid', self::optionsToAttrs($options), $nodes);
    }

    /** @param LpdfNode[] $nodes */
    public static function frame(array $nodes = [], ?FrameOptions $options = null): LpdfContainerNode
    {
        return new LpdfContainerNode('frame', self::optionsToAttrs($options), $nodes);
    }

    /** @param LpdfNode[] $nodes */
    public static function link(array $nodes = [], ?LinkOptions $options = null): LpdfContainerNode
    {
        return new LpdfContainerNode('link', self::optionsToAttrs($options), $nodes);
    }

    // ── Leaf helpers ──────────────────────────────────────────────────────────

    /**
     * Build a text paragraph node.
     *
     * @param  array<string|LpdfSpanNode> $nodes Children must be strings or LpdfSpanNode instances.
     * @throws \InvalidArgumentException         if a child is neither a string nor a LpdfSpanNode.
     */
    public static function text(array $nodes = [], ?TextOptions $options = null): LpdfTextNode
    {
        foreach ($nodes as $i => $child) {
            if (!is_string($child) && !$child instanceof LpdfSpanNode) {
                throw new \InvalidArgumentException(
                    "text() child at index $i must be a string or LpdfSpanNode, got " . get_debug_type($child),
                );
            }
        }

        return new LpdfTextNode(self::optionsToAttrs($options), $nodes);
    }

    /**
     * Build a span inline node.
     *
     * @param  string[] $nodes Children must be plain strings.
     * @throws \InvalidArgumentException if a child is not a string.
     */
    public static function span(array $nodes = [], ?SpanOptions $options = null): LpdfSpanNode
    {
        foreach ($nodes as $i => $child) {
            if (!is_string($child)) {
                throw new \InvalidArgumentException(
                    "span() child at index $i must be a string, got " . get_debug_type($child),
                );
            }
        }

        return new LpdfSpanNode(self::optionsToAttrs($options), $nodes);
    }

    /** Build a divider (horizontal rule) node. */
    public static function divider(?DividerOptions $options = null): LpdfDividerNode
    {
        return new LpdfDividerNode(self::optionsToAttrs($options));
    }

    // ── Page + document ───────────────────────────────────────────────────────

    /** @param LpdfNode[] $nodes */
    public static function page(array $nodes = [], ?PageOptions $options = null): LpdfPageNode
    {
        return new LpdfPageNode(self::optionsToAttrs($options), $nodes);
    }

    /**
     * Build the root document node, ready for {@see LpdfEngine::renderPdf()}.
     *
     * @param LpdfPageNode[] $nodes
     */
    public static function document(array $nodes = [], ?DocumentOptions $options = null): LpdfDocument
    {
        // tokens and meta are sub-objects, not flat string attrs — handle separately.
        $attrs = self::optionsToAttrs($options, skip: ['tokens', 'meta']);

        if ($options?->tokens !== null) {
            $attrs['tokens'] = $options->tokens;
        }
        if ($options?->meta !== null) {
            $attrs['meta'] = $options->meta;
        }

        return new LpdfDocument($attrs, $nodes);
    }

    // ── Private helpers ───────────────────────────────────────────────────────

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
