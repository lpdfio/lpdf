<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

/** @internal Use Canvas::text() to construct. */
final readonly class TextNode extends Node
{
    /** @param Run[] $runs */
    public function __construct(
        private float      $x,
        private float      $y,
        private string     $content,
        private ?TextStyle $style = null,
        private array      $runs  = [],
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['x' => (string)$this->x, 'y' => (string)$this->y];
        if ($this->style?->font       !== null) $attrs['font']        = $this->style->font;
        if ($this->style?->size       !== null) $attrs['font-size']   = (string)$this->style->size;
        if ($this->style?->color      !== null) $attrs['color']       = $this->style->color;
        if ($this->style?->align      !== null) $attrs['align']       = $this->style->align->value;
        if ($this->style?->lineHeight !== null) $attrs['line-height'] = (string)$this->style->lineHeight;
        if ($this->style?->width      !== null) $attrs['w']           = (string)$this->style->width;

        $node = ['type' => 'canvas-text', 'text' => $this->content, 'attrs' => (object) $attrs];
        if ($this->runs !== []) {
            $node['runs'] = array_map(static function (Run $r): array {
                $runAttrs = [];
                if ($r->font  !== null) $runAttrs['font']       = $r->font;
                if ($r->size  !== null) $runAttrs['font-size'] = (string)$r->size;
                if ($r->color !== null) $runAttrs['color']     = $r->color;
                return ['text' => $r->text, 'attrs' => (object) $runAttrs];
            }, $this->runs);
        }
        return $node;
    }
}
