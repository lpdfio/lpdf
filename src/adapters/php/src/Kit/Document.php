<?php

declare(strict_types=1);

namespace Lpdf\Kit;

/**
 * Root document node — pass to {@see \Lpdf\Engine::renderPdf()}.
 *
 * @internal Use Kit::document() to construct.
 */
final readonly class Document implements \JsonSerializable
{
    /**
     * @param array<string,mixed>  $attrs     Flat string attrs plus optional 'tokens' and 'meta' sub-objects.
     * @param SectionNode[]        $nodes
     */
    public function __construct(
        private array $attrs,
        private array $nodes,
    ) {}

    public function jsonSerialize(): mixed
    {
        return [
            'version'  => 1,
            'type'     => 'document',
            'attrs'    => (object) $this->attrs,
            'nodes'    => $this->nodes,
        ];
    }
}
