<?php

declare(strict_types=1);

namespace Lpdf\Layout;

/**
 * A form field leaf node.
 *
 * @internal Use Layout::field() to construct.
 */
final readonly class FieldNode extends Node
{
    /**
     * @param array<string,string> $attrs  Must include 'type' and 'name'.
     */
    public function __construct(private array $attrs) {}

    public function jsonSerialize(): mixed
    {
        return [
            'type'  => 'field',
            'attrs' => (object) $this->attrs,
        ];
    }
}
