<?php

declare(strict_types=1);

namespace Lpdf\Layout;

/**
 * A layout-region node for pinned/absolute layout regions.
 *
 * @internal Use Layout::region() to construct.
 */
final readonly class RegionNode extends Node
{
    /**
     * @param array<string,string> $attrs   Must include 'pin'.
     * @param Node[]               $nodes
     */
    public function __construct(
        private array $attrs,
        private array $nodes,
    ) {}

    public function jsonSerialize(): mixed
    {
        return [
            'type'  => 'layout-region',
            'attrs' => (object) $this->attrs,
            'nodes' => $this->nodes,
        ];
    }
}
