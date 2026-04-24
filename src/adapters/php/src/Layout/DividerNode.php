<?php

declare(strict_types=1);

namespace Lpdf\Layout;

/**
 * A divider (horizontal rule) node.
 *
 * @internal Use Layout::divider() to construct.
 */
final readonly class DividerNode extends Node
{
    /** @param array<string,string> $attrs */
    public function __construct(private array $attrs) {}

    public function getType(): string { return 'divider'; }

    public function jsonSerialize(): mixed
    {
        return [
            'type'  => 'divider',
            'attrs' => (object) $this->attrs,
        ];
    }
}
