<?php

declare(strict_types=1);

namespace Lpdf\Layout;

/**
 * A text paragraph node.
 *
 * @internal Use Layout::text() to construct.
 */
final readonly class TextNode extends Node
{
    /**
     * @param array<string,string>       $attrs
     * @param array<string|SpanNode>     $nodes
     */
    public function __construct(
        private array $attrs,
        private array $nodes,
    ) {}

    public function jsonSerialize(): mixed
    {
        return [
            'type'  => 'text',
            'attrs' => (object) $this->attrs,
            'nodes' => $this->nodes,
        ];
    }
}
