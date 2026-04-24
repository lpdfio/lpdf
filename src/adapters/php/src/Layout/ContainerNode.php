<?php

declare(strict_types=1);

namespace Lpdf\Layout;

/** @internal Use Layout factory methods to construct. */
final readonly class ContainerNode extends Node
{
    /**
     * @param string               $type
     * @param array<string,string> $attrs
     * @param Node[]               $nodes
     */
    public function __construct(
        private string $type,
        private array  $attrs,
        private array  $nodes,
    ) {}

    public function jsonSerialize(): mixed
    {
        return [
            'type'  => $this->type,
            'attrs' => (object) $this->attrs,
            'nodes' => $this->nodes,
        ];
    }
}
