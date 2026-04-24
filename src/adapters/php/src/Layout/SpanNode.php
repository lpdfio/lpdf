<?php

declare(strict_types=1);

namespace Lpdf\Layout;

/**
 * A span inline node.
 *
 * @internal Use Layout::span() to construct.
 */
final readonly class SpanNode extends Node
{
    /**
     * @param array<string,string> $attrs
     * @param string[]             $children
     */
    public function __construct(
        private array $attrs,
        private array $children,
    ) {}

    public function getType(): string { return 'span'; }

    public function jsonSerialize(): mixed
    {
        return [
            'type'  => 'span',
            'attrs' => (object) $this->attrs,
            'nodes' => $this->children,
        ];
    }
}
