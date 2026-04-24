<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

use Lpdf\Shared\PageScope;

/** @internal Use Canvas::layer() to construct. */
final readonly class LayerNode extends Node
{
    /** @param Node[] $nodes */
    public function __construct(
        private array         $nodes,
        private ?LayerOptions $options = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = [];
        if ($this->options?->page      !== null) $attrs['page']      = $this->options->page instanceof PageScope ? $this->options->page->value : $this->options->page;
        if ($this->options?->opacity   !== null) $attrs['opacity']   = (string)$this->options->opacity;
        if ($this->options?->transform !== null) $attrs['transform'] = $this->options->transform->matrix;
        if ($this->options?->clip      !== null) {
            $c = $this->options->clip;
            $clipArr = ['x' => $c->x, 'y' => $c->y, 'w' => $c->w, 'h' => $c->h];
            if ($c->borderRadius !== 0.0) $clipArr['borderRadius'] = $c->borderRadius;
            $attrs['clip'] = $clipArr;
        }
        return [
            'type'  => 'canvas-layer',
            'attrs' => (object) $attrs,
            'nodes' => $this->nodes,
        ];
    }
}
