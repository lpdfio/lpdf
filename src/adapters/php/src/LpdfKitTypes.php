<?php

declare(strict_types=1);

namespace Lpdf;

// ── Options ───────────────────────────────────────────────────────────────────

final readonly class StackOptions
{
    public function __construct(
        public ?string $gap        = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $align      = null,
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $border     = null,
        public ?string $radius     = null,
    ) {}
}

final readonly class FlankOptions
{
    public function __construct(
        public ?string $gap        = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $align      = null,
        public ?string $justify    = null,
        public ?string $wrap       = null,
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $border     = null,
        public ?string $radius     = null,
    ) {}
}

final readonly class SplitOptions
{
    public function __construct(
        public ?string $gap        = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $align      = null,
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $border     = null,
        public ?string $radius     = null,
    ) {}
}

final readonly class ClusterOptions
{
    public function __construct(
        public ?string $gap        = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $align      = null,
        public ?string $justify    = null,
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $border     = null,
        public ?string $radius     = null,
    ) {}
}

final readonly class GridOptions
{
    public function __construct(
        public ?string $cols       = null,
        public ?string $colWidth   = null,
        public ?string $gap        = null,
        public ?string $equal      = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $border     = null,
        public ?string $radius     = null,
    ) {}
}

final readonly class FrameOptions
{
    public function __construct(
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $border     = null,
        public ?string $radius     = null,
        public ?string $align      = null,
    ) {}
}

final readonly class LinkOptions
{
    public function __construct(
        public ?string $url = null,
    ) {}
}

final readonly class TextOptions
{
    public function __construct(
        public ?string $font       = null,
        public ?string $fontSize   = null,
        public ?string $textAlign  = null,
        public ?string $color      = null,
        public ?string $bold       = null,
        public ?string $end        = null,
        public ?string $repeat     = null,
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $border     = null,
        public ?string $radius     = null,
    ) {}
}

final readonly class SpanOptions
{
    public function __construct(
        public ?string $font      = null,
        public ?string $fontSize  = null,
        public ?string $color     = null,
        public ?string $bold      = null,
        public ?string $url       = null,
        public ?string $underline = null,
        public ?string $strike    = null,
    ) {}
}

final readonly class DividerOptions
{
    public function __construct(
        public ?string $color     = null,
        public ?string $thickness = null,
        public ?string $direction = null,
    ) {}
}

final readonly class PageOptions
{
    public function __construct(
        public ?string $size        = null,
        public ?string $orientation = null,
        public ?string $margin      = null,
        public ?string $background  = null,
    ) {}
}

// ── Tokens + meta ─────────────────────────────────────────────────────────────

/** PDF document metadata written into the output file. */
final readonly class LpdfMeta implements \JsonSerializable
{
    public function __construct(
        public ?string $title    = null,
        public ?string $author   = null,
        public ?string $subject  = null,
        public ?string $keywords = null,
        public ?string $creator  = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        return array_filter(
            [
                'title'    => $this->title,
                'author'   => $this->author,
                'subject'  => $this->subject,
                'keywords' => $this->keywords,
                'creator'  => $this->creator,
            ],
            static fn($v) => $v !== null,
        );
    }
}

/**
 * Design-token overrides applied to the whole document.
 *
 * @phpstan-type FontDef array{src: string}|array{builtin: string}
 */
final readonly class LpdfTokens implements \JsonSerializable
{
    /**
     * @param array<string,string>|null $colors
     * @param array<string,string>|null $space
     * @param array<string,string>|null $grid
     * @param array<string,string>|null $border
     * @param array<string,string>|null $radius
     * @param array<string,string>|null $width
     * @param array<string,string>|null $text
     * @param array<string,array{src?:string,builtin?:string}>|null $fonts
     */
    public function __construct(
        public ?array $colors = null,
        public ?array $space  = null,
        public ?array $grid   = null,
        public ?array $border = null,
        public ?array $radius = null,
        public ?array $width  = null,
        public ?array $text   = null,
        public ?array $fonts  = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        return array_filter(
            [
                'colors' => $this->colors,
                'space'  => $this->space,
                'grid'   => $this->grid,
                'border' => $this->border,
                'radius' => $this->radius,
                'width'  => $this->width,
                'text'   => $this->text,
                'fonts'  => $this->fonts,
            ],
            static fn($v) => $v !== null,
        );
    }
}

final readonly class DocumentOptions
{
    public function __construct(
        public ?string     $size        = null,
        public ?string     $orientation = null,
        public ?string     $margin      = null,
        public ?string     $background  = null,
        public ?LpdfTokens $tokens      = null,
        public ?LpdfMeta   $meta        = null,
    ) {}
}

// ── Nodes ─────────────────────────────────────────────────────────────────────

/** Base class for all lpdf layout nodes. Use {@see LpdfKit} factory methods to construct. */
abstract class LpdfNode implements \JsonSerializable
{
    abstract public function getType(): string;
}

/**
 * A layout container node (stack, flank, split, cluster, grid, frame, link).
 *
 * @internal Use LpdfKit factory methods to construct.
 */
final readonly class LpdfContainerNode extends LpdfNode
{
    /**
     * @param string               $type
     * @param array<string,string> $attrs
     * @param LpdfNode[]           $children
     */
    public function __construct(
        private string $type,
        private array  $attrs,
        private array  $children,
    ) {}

    public function getType(): string { return $this->type; }

    public function jsonSerialize(): mixed
    {
        return [
            'type'     => $this->type,
            'attrs'    => (object) $this->attrs,
            'children' => $this->children,
        ];
    }
}

/**
 * A page node.
 *
 * @internal Use LpdfKit::page() to construct.
 */
final readonly class LpdfPageNode extends LpdfNode
{
    /**
     * @param array<string,string> $attrs
     * @param LpdfNode[]           $children
     */
    public function __construct(
        private array $attrs,
        private array $children,
    ) {}

    public function getType(): string { return 'page'; }

    public function jsonSerialize(): mixed
    {
        return [
            'type'     => 'page',
            'attrs'    => (object) $this->attrs,
            'children' => $this->children,
        ];
    }
}

/**
 * A text paragraph node. Children may be plain strings or {@see LpdfSpanNode} instances.
 *
 * @internal Use LpdfKit::text() to construct.
 */
final readonly class LpdfTextNode extends LpdfNode
{
    /**
     * @param array<string,string>       $attrs
     * @param array<string|LpdfSpanNode> $children
     */
    public function __construct(
        private array $attrs,
        private array $children,
    ) {}

    public function getType(): string { return 'text'; }

    public function jsonSerialize(): mixed
    {
        return [
            'type'     => 'text',
            'attrs'    => (object) $this->attrs,
            'children' => $this->children,
        ];
    }
}

/**
 * A span inline node. Children are plain strings only.
 *
 * @internal Use LpdfKit::span() to construct.
 */
final readonly class LpdfSpanNode extends LpdfNode
{
    /**
     * @param array<string,string> $attrs
     * @param string[]             $children
     */
    public function __construct(
        private array $attrs,
        private array $children,
    ) {}

    public function getType(): string { return 'span'; }

    public function jsonSerialize(): mixed
    {
        return [
            'type'     => 'span',
            'attrs'    => (object) $this->attrs,
            'children' => $this->children,
        ];
    }
}

/**
 * A divider (horizontal rule) node.
 *
 * @internal Use LpdfKit::divider() to construct.
 */
final readonly class LpdfDividerNode extends LpdfNode
{
    /**
     * @param array<string,string> $attrs
     */
    public function __construct(
        private array $attrs,
    ) {}

    public function getType(): string { return 'divider'; }

    public function jsonSerialize(): mixed
    {
        return [
            'type'  => 'divider',
            'attrs' => (object) $this->attrs,
        ];
    }
}

/**
 * Root document node — pass to {@see LpdfEngine::renderPdf()}.
 *
 * @internal Use LpdfKit::document() to construct.
 */
final readonly class LpdfDocument implements \JsonSerializable
{
    /**
     * @param array<string,mixed> $attrs    Flat string attrs plus optional 'tokens' and 'meta' sub-objects.
     * @param LpdfPageNode[]      $children
     */
    public function __construct(
        private array $attrs,
        private array $children,
    ) {}

    public function jsonSerialize(): mixed
    {
        return [
            'version'  => 1,
            'type'     => 'document',
            'attrs'    => (object) $this->attrs,
            'children' => $this->children,
        ];
    }
}
