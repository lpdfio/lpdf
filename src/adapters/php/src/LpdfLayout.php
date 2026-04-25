<?php

declare(strict_types=1);

namespace Lpdf;

use Lpdf\Shared\AttrsHelper;
use Lpdf\Layout\BarcodeNode;
use Lpdf\Layout\BarcodeOptions;
use Lpdf\Layout\ClusterOptions;
use Lpdf\Layout\ContainerNode;
use Lpdf\Layout\DividerNode;
use Lpdf\Layout\DividerOptions;
use Lpdf\Layout\FlankOptions;
use Lpdf\Layout\FrameOptions;
use Lpdf\Layout\GridOptions;
use Lpdf\Layout\FieldNode;
use Lpdf\Layout\FieldOptions;
use Lpdf\Layout\FieldType;
use Lpdf\Layout\ImgNode;
use Lpdf\Layout\ImgOptions;
use Lpdf\Layout\LinkOptions;
use Lpdf\Layout\Node;
use Lpdf\Layout\Pin;
use Lpdf\Layout\RegionNode;
use Lpdf\Layout\RegionOptions;
use Lpdf\Layout\SpanNode;
use Lpdf\Layout\SpanOptions;
use Lpdf\Layout\SplitOptions;
use Lpdf\Layout\StackOptions;
use Lpdf\Layout\TableOptions;
use Lpdf\Layout\TdOptions;
use Lpdf\Layout\TextNode;
use Lpdf\Layout\TextOptions;
use Lpdf\Layout\TheadOptions;
use Lpdf\Layout\TrOptions;

/**
 * Static factory for flow/layout primitives.
 *
 * All methods return serialisable node objects. Pass the result of
 * {@see Kit::layout()} (wrapping these nodes) to {@see Kit::section()},
 * and finally to {@see Engine::renderPdf()}.
 */
final class LpdfLayout
{
    use AttrsHelper;
    // ── Container helpers ─────────────────────────────────────────────────────

    /** @param Node[] $nodes */
    public static function stack(array $nodes = [], ?StackOptions $options = null): ContainerNode
    {
        return new ContainerNode('stack', self::optionsToAttrs($options), $nodes);
    }

    /** @param Node[] $nodes */
    public static function flank(array $nodes = [], ?FlankOptions $options = null): ContainerNode
    {
        return new ContainerNode('flank', self::optionsToAttrs($options), $nodes);
    }

    /** @param Node[] $nodes */
    public static function split(array $nodes = [], ?SplitOptions $options = null): ContainerNode
    {
        return new ContainerNode('split', self::optionsToAttrs($options), $nodes);
    }

    /** @param Node[] $nodes */
    public static function cluster(array $nodes = [], ?ClusterOptions $options = null): ContainerNode
    {
        return new ContainerNode('cluster', self::optionsToAttrs($options), $nodes);
    }

    /** @param Node[] $nodes */
    public static function grid(array $nodes = [], ?GridOptions $options = null): ContainerNode
    {
        return new ContainerNode('grid', self::optionsToAttrs($options), $nodes);
    }

    /** @param Node[] $nodes */
    public static function frame(array $nodes = [], ?FrameOptions $options = null): ContainerNode
    {
        return new ContainerNode('frame', self::optionsToAttrs($options), $nodes);
    }

    /** @param Node[] $nodes */
    public static function link(array $nodes = [], ?LinkOptions $options = null): ContainerNode
    {
        return new ContainerNode('link', self::optionsToAttrs($options), $nodes);
    }

    // ── Leaf helpers ──────────────────────────────────────────────────────────

    /**
     * Build a text paragraph node.
     *
     * @param  array<string|SpanNode> $nodes Children must be strings or SpanNode instances.
     * @throws \InvalidArgumentException      if a child is neither a string nor a SpanNode.
     */
    public static function text(array $nodes = [], ?TextOptions $options = null): TextNode
    {
        foreach ($nodes as $i => $child) {
            if (!is_string($child) && !$child instanceof SpanNode) {
                throw new \InvalidArgumentException(
                    "text() child at index $i must be a string or SpanNode, got " . get_debug_type($child),
                );
            }
        }

        return new TextNode(self::optionsToAttrs($options), $nodes);
    }

    /**
     * Build a span inline node.
     *
     * @param  string[] $nodes Children must be plain strings.
     * @throws \InvalidArgumentException if a child is not a string.
     */
    public static function span(array $nodes = [], ?SpanOptions $options = null): SpanNode
    {
        foreach ($nodes as $i => $child) {
            if (!is_string($child)) {
                throw new \InvalidArgumentException(
                    "span() child at index $i must be a string, got " . get_debug_type($child),
                );
            }
        }

        return new SpanNode(self::optionsToAttrs($options), $nodes);
    }

    /** Build a divider (horizontal rule) node. */
    public static function divider(?DividerOptions $options = null): DividerNode
    {
        return new DividerNode(self::optionsToAttrs($options));
    }

    /** Build an img (image) node. */
    public static function img(ImgOptions $options): ImgNode
    {
        return new ImgNode(self::optionsToAttrs($options));
    }

    /** Build a barcode node. */
    public static function barcode(BarcodeOptions $options): BarcodeNode
    {
        return new BarcodeNode(self::optionsToAttrs($options));
    }

    /**
     * Build an interactive form field node.
     *
     * @param  FieldType|string $type   Field type — {@see FieldType}.
     * @param  string           $name   Unique field name within the document.
     */
    public static function field(
        FieldType|string $type,
        string           $name,
        ?FieldOptions    $options = null,
    ): FieldNode {
        $attrs = array_merge(
            ['type' => $type instanceof FieldType ? $type->value : $type, 'name' => $name],
            self::optionsToAttrs($options),
        );
        return new FieldNode($attrs);
    }

    // ── Table helpers ─────────────────────────────────────────────────────────

    /** @param ContainerNode[] $nodes */
    public static function table(array $nodes = [], ?TableOptions $options = null): ContainerNode
    {
        return new ContainerNode('table', self::optionsToAttrs($options), $nodes);
    }

    /** @param ContainerNode[] $nodes */
    public static function thead(array $nodes = [], ?TheadOptions $options = null): ContainerNode
    {
        return new ContainerNode('thead', self::optionsToAttrs($options), $nodes);
    }

    /** @param ContainerNode[] $nodes */
    public static function tr(array $nodes = [], ?TrOptions $options = null): ContainerNode
    {
        return new ContainerNode('tr', self::optionsToAttrs($options), $nodes);
    }

    /** @param Node[] $nodes */
    public static function td(array $nodes = [], ?TdOptions $options = null): ContainerNode
    {
        return new ContainerNode('td', self::optionsToAttrs($options), $nodes);
    }

    // ── Region ────────────────────────────────────────────────────────────────

    /**
     * Build a pinned layout-region node.
     *
     * @param  Pin|string   $pin    Anchor pin — {@see Pin::Top}, {@see Pin::Bottom}, {@see Pin::Left}, {@see Pin::Right}.
     * @param  Node[]       $nodes  Layout nodes placed in the region.
     */
    public static function region(
        Pin|string      $pin,
        array           $nodes   = [],
        ?RegionOptions  $options = null,
    ): RegionNode {
        $attrs = array_merge(['pin' => $pin instanceof Pin ? $pin->value : $pin], self::optionsToAttrs($options));
        return new RegionNode($attrs, $nodes);
    }

}
