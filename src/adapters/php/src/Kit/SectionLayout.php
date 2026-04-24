<?php

declare(strict_types=1);

namespace Lpdf\Kit;

use Lpdf\Layout\Node;

/**
 * A layout block inside a section — wraps layout nodes.
 *
 * @internal Use Kit::layout() to construct.
 */
final readonly class SectionLayout implements \JsonSerializable
{
    /** @param Node[] $nodes */
    public function __construct(private array $nodes) {}

    public function jsonSerialize(): mixed
    {
        return ['type' => 'layout', 'nodes' => $this->nodes];
    }
}
