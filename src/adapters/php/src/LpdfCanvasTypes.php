<?php

declare(strict_types=1);

namespace Lpdf;

// ── Style / option value-objects ──────────────────────────────────────────────

final readonly class CanvasTextStyle
{
    public function __construct(
        public ?string $font        = null,
        public ?float  $size        = null,
        public ?string $color       = null,
        public ?string $align       = null,  // left | center | right | justify
        public ?float  $lineHeight  = null,
        public ?float  $width       = null,
    ) {}
}

final readonly class CanvasRectStyle
{
    public function __construct(
        public ?string      $fill          = null,
        public ?string      $stroke        = null,
        public ?float       $strokeWidth   = null,
        /** @var float[]|null */
        public ?array       $strokeDash    = null,
        public ?float       $borderRadius  = null,
    ) {}
}

final readonly class CanvasLineStyle
{
    public function __construct(
        public ?string      $stroke       = null,
        public ?float       $strokeWidth  = null,
        /** @var float[]|null */
        public ?array       $strokeDash   = null,
        public ?string      $lineCap      = null,  // butt | round | square
        public ?string      $lineJoin     = null,  // miter | round | bevel
    ) {}
}

final readonly class CanvasEllipseStyle
{
    public function __construct(
        public ?string      $fill         = null,
        public ?string      $stroke       = null,
        public ?float       $strokeWidth  = null,
        /** @var float[]|null */
        public ?array       $strokeDash   = null,
    ) {}
}

final readonly class CanvasPathStyle
{
    public function __construct(
        public ?string      $fill              = null,
        public ?string      $stroke            = null,
        public ?float       $strokeWidth       = null,
        /** @var float[]|null */
        public ?array       $strokeDash        = null,
        public ?bool        $fillRuleEvenodd   = null,
        public ?string      $lineCap           = null,  // butt | round | square
        public ?string      $lineJoin          = null,  // miter | round | bevel
    ) {}
}

final readonly class CanvasTransform
{
    /**
     * Affine transform matrix [a, b, c, d, e, f] (SVG / PDF convention).
     * @param float[] $matrix
     */
    public function __construct(public array $matrix) {}
}

final readonly class CanvasClip
{
    public function __construct(
        public float  $x,
        public float  $y,
        public float  $w,
        public float  $h,
        public float  $borderRadius = 0.0,
    ) {}
}

final readonly class CanvasLayerOptions
{
    public function __construct(
        public ?float         $opacity   = null,
        public ?CanvasTransform $transform = null,
        public ?CanvasClip    $clip      = null,
    ) {}
}

/** A rich-text run for {@see LpdfCanvas::text()}. */
final readonly class CanvasRun
{
    public function __construct(
        public string  $text,
        public ?string $font  = null,
        public ?float  $size  = null,
        public ?string $color = null,
    ) {}
}

// ── Canvas node classes ───────────────────────────────────────────────────────

/** Base class for all canvas nodes. */
abstract readonly class LpdfCanvasNode implements \JsonSerializable {}

final readonly class LpdfCanvasTextNode extends LpdfCanvasNode
{
    /** @param CanvasRun[] $runs */
    public function __construct(
        private float   $x,
        private float   $y,
        private string  $content,
        private ?CanvasTextStyle $style = null,
        private array   $runs = [],
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['x' => $this->x, 'y' => $this->y, 'content' => $this->content];
        if ($this->style?->font       !== null) $attrs['font']       = $this->style->font;
        if ($this->style?->size       !== null) $attrs['size']       = $this->style->size;
        if ($this->style?->color      !== null) $attrs['color']      = $this->style->color;
        if ($this->style?->align      !== null) $attrs['align']      = $this->style->align;
        if ($this->style?->lineHeight !== null) $attrs['line-height'] = $this->style->lineHeight;
        if ($this->style?->width      !== null) $attrs['width']      = $this->style->width;

        $node = ['type' => 'canvas-text', 'attrs' => (object) $attrs];
        if ($this->runs !== []) {
            $node['runs'] = array_map(static function (CanvasRun $r): array {
                $run = ['text' => $r->text];
                if ($r->font  !== null) $run['font']  = $r->font;
                if ($r->size  !== null) $run['size']  = $r->size;
                if ($r->color !== null) $run['color'] = $r->color;
                return $run;
            }, $this->runs);
        }
        return $node;
    }
}

final readonly class LpdfCanvasRectNode extends LpdfCanvasNode
{
    public function __construct(
        private float  $x,
        private float  $y,
        private float  $w,
        private float  $h,
        private ?CanvasRectStyle $style = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['x' => $this->x, 'y' => $this->y, 'w' => $this->w, 'h' => $this->h];
        if ($this->style?->fill         !== null) $attrs['fill']         = $this->style->fill;
        if ($this->style?->stroke       !== null) $attrs['stroke']       = $this->style->stroke;
        if ($this->style?->strokeWidth  !== null) $attrs['strokeWidth']  = $this->style->strokeWidth;
        if ($this->style?->strokeDash   !== null) $attrs['strokeDash']   = $this->style->strokeDash;
        if ($this->style?->borderRadius !== null) $attrs['borderRadius'] = $this->style->borderRadius;
        return ['type' => 'canvas-rect', 'attrs' => (object) $attrs];
    }
}

final readonly class LpdfCanvasLineNode extends LpdfCanvasNode
{
    public function __construct(
        private float  $x1,
        private float  $y1,
        private float  $x2,
        private float  $y2,
        private ?CanvasLineStyle $style = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['x1' => $this->x1, 'y1' => $this->y1, 'x2' => $this->x2, 'y2' => $this->y2];
        if ($this->style?->stroke      !== null) $attrs['stroke']      = $this->style->stroke;
        if ($this->style?->strokeWidth !== null) $attrs['strokeWidth'] = $this->style->strokeWidth;
        if ($this->style?->strokeDash  !== null) $attrs['strokeDash']  = $this->style->strokeDash;
        if ($this->style?->lineCap     !== null) $attrs['lineCap']     = $this->style->lineCap;
        if ($this->style?->lineJoin    !== null) $attrs['lineJoin']    = $this->style->lineJoin;
        return ['type' => 'canvas-line', 'attrs' => (object) $attrs];
    }
}

final readonly class LpdfCanvasEllipseNode extends LpdfCanvasNode
{
    public function __construct(
        private float  $cx,
        private float  $cy,
        private float  $rx,
        private float  $ry,
        private ?CanvasEllipseStyle $style = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['cx' => $this->cx, 'cy' => $this->cy, 'rx' => $this->rx, 'ry' => $this->ry];
        if ($this->style?->fill        !== null) $attrs['fill']        = $this->style->fill;
        if ($this->style?->stroke      !== null) $attrs['stroke']      = $this->style->stroke;
        if ($this->style?->strokeWidth !== null) $attrs['strokeWidth'] = $this->style->strokeWidth;
        if ($this->style?->strokeDash  !== null) $attrs['strokeDash']  = $this->style->strokeDash;
        return ['type' => 'canvas-ellipse', 'attrs' => (object) $attrs];
    }
}

final readonly class LpdfCanvasCircleNode extends LpdfCanvasNode
{
    public function __construct(
        private float  $cx,
        private float  $cy,
        private float  $r,
        private ?CanvasEllipseStyle $style = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['cx' => $this->cx, 'cy' => $this->cy, 'r' => $this->r];
        if ($this->style?->fill        !== null) $attrs['fill']        = $this->style->fill;
        if ($this->style?->stroke      !== null) $attrs['stroke']      = $this->style->stroke;
        if ($this->style?->strokeWidth !== null) $attrs['strokeWidth'] = $this->style->strokeWidth;
        if ($this->style?->strokeDash  !== null) $attrs['strokeDash']  = $this->style->strokeDash;
        return ['type' => 'canvas-circle', 'attrs' => (object) $attrs];
    }
}

final readonly class LpdfCanvasPathNode extends LpdfCanvasNode
{
    public function __construct(
        private string $d,
        private ?CanvasPathStyle $style = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['d' => $this->d];
        if ($this->style?->fill            !== null) $attrs['fill']            = $this->style->fill;
        if ($this->style?->stroke          !== null) $attrs['stroke']          = $this->style->stroke;
        if ($this->style?->strokeWidth     !== null) $attrs['strokeWidth']     = $this->style->strokeWidth;
        if ($this->style?->strokeDash      !== null) $attrs['strokeDash']      = $this->style->strokeDash;
        if ($this->style?->fillRuleEvenodd !== null) $attrs['fillRuleEvenodd'] = $this->style->fillRuleEvenodd;
        if ($this->style?->lineCap         !== null) $attrs['lineCap']         = $this->style->lineCap;
        if ($this->style?->lineJoin        !== null) $attrs['lineJoin']        = $this->style->lineJoin;
        return ['type' => 'canvas-path', 'attrs' => (object) $attrs];
    }
}

final readonly class LpdfCanvasImageNode extends LpdfCanvasNode
{
    public function __construct(
        private float  $x,
        private float  $y,
        private float  $w,
        private float  $h,
        private string $src,
    ) {}

    public function jsonSerialize(): mixed
    {
        return [
            'type'  => 'canvas-image',
            'attrs' => (object) ['x' => $this->x, 'y' => $this->y, 'w' => $this->w, 'h' => $this->h, 'src' => $this->src],
        ];
    }
}

final readonly class LpdfCanvasLayerNode extends LpdfCanvasNode
{
    /** @param LpdfCanvasNode[] $children */
    public function __construct(
        private array               $children,
        private ?CanvasLayerOptions $options = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = [];
        if ($this->options?->opacity   !== null) $attrs['opacity']   = $this->options->opacity;
        if ($this->options?->transform !== null) $attrs['transform'] = $this->options->transform->matrix;
        if ($this->options?->clip      !== null) {
            $c = $this->options->clip;
            $clipArr = ['x' => $c->x, 'y' => $c->y, 'w' => $c->w, 'h' => $c->h];
            if ($c->borderRadius !== 0.0) $clipArr['borderRadius'] = $c->borderRadius;
            $attrs['clip'] = $clipArr;
        }
        return [
            'type'     => 'canvas-layer',
            'attrs'    => (object) $attrs,
            'children' => $this->children,
        ];
    }
}

// ── Canvas page and document ──────────────────────────────────────────────────

final readonly class CanvasPageOptions
{
    public function __construct(
        public ?float  $width      = null,
        public ?float  $height     = null,
        public ?string $size       = null,   // e.g. 'a4', 'letter'
        public ?string $orientation = null,  // 'landscape'
        public ?string $margin     = null,
        public ?string $background = null,
    ) {}
}

/** A canvas page. */
final readonly class LpdfCanvasPage implements \JsonSerializable
{
    /** @param LpdfCanvasNode[] $children */
    public function __construct(
        private array              $children,
        private ?CanvasPageOptions $options = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = [];
        if ($this->options?->width       !== null) $attrs['width']       = $this->options->width;
        if ($this->options?->height      !== null) $attrs['height']      = $this->options->height;
        if ($this->options?->size        !== null) $attrs['size']        = $this->options->size;
        if ($this->options?->orientation !== null) $attrs['orientation'] = $this->options->orientation;
        if ($this->options?->margin      !== null) $attrs['margin']      = $this->options->margin;
        if ($this->options?->background  !== null) $attrs['background']  = $this->options->background;
        return [
            'type'     => 'canvas-page',
            'attrs'    => (object) $attrs,
            'children' => $this->children,
        ];
    }
}

/**
 * Root canvas document — pass to {@see LpdfEngine::renderPdf()}.
 *
 * Implements JsonSerializable so it can be JSON-encoded directly.
 */
final readonly class LpdfCanvasDocument implements \JsonSerializable
{
    /**
     * @param LpdfCanvasPage[] $pages
     * @param ?LpdfMeta        $meta
     * @param array<string, array{core?: string, ref?: string}> $fonts
     * @param array<string, array{ref?: string}>                $images
     */
    public function __construct(
        private array     $pages,
        private ?LpdfMeta $meta   = null,
        private array     $fonts  = [],
        private array     $images = [],
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = [];
        if ($this->meta !== null) {
            $attrs['meta'] = $this->meta;
        }
        if ($this->fonts !== [] || $this->images !== []) {
            $assets = [];
            if ($this->fonts  !== []) $assets['fonts']  = $this->fonts;
            if ($this->images !== []) $assets['images'] = $this->images;
            $attrs['assets'] = $assets;
        }

        return [
            'version' => 1,
            'type'    => 'canvas-document',
            'attrs'   => (object) $attrs,
            'pages'   => $this->pages,
        ];
    }
}
