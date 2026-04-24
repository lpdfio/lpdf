<?php

declare(strict_types=1);

namespace Lpdf\Kit;

use Lpdf\Canvas\LayerNode;

/**
 * A canvas block inside a section — wraps canvas layers.
 *
 * @internal Use Kit::canvas() to construct.
 */
final readonly class SectionCanvas implements \JsonSerializable
{
    /** @param LayerNode[] $layers */
    public function __construct(private array $layers) {}

    public function jsonSerialize(): mixed
    {
        return ['type' => 'canvas', 'nodes' => $this->layers];
    }
}
