<?php

declare(strict_types=1);

namespace Lpdf\Kit;

/**
 * Design-token overrides applied to the whole document.
 *
 * @phpstan-type FontDef array{src: string}|array{builtin: string}
 */
final readonly class DocumentTokens implements \JsonSerializable
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
