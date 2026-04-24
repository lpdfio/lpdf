<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class FieldOptions
{
    public function __construct(
        /** Displayed label or button caption. */
        public ?string    $label     = null,
        /** Initial value for text/dropdown fields. */
        public ?string    $value     = null,
        /** Comma-separated option list for dropdown/radio. */
        public ?string    $options   = null,
        /** Radio-group name. */
        public ?string    $group     = null,
        /** Initial checked state for checkbox/radio. */
        public ?bool      $checked   = null,
        public ?bool      $required  = null,
        public ?bool      $readonly  = null,
        /** Maximum character count for text fields. */
        public ?string    $maxLen    = null,
        /** Submit URL for button fields. */
        public ?string    $actionUrl = null,
        public ?string    $width     = null,
        public ?string    $height    = null,
        public ?bool      $debug     = null,
        // Data-binding attrs
        public ?string    $dataValue  = null,
        public ?string    $dataSource = null,
        public ?string    $dataIf     = null,
        public ?string    $dataIfNot  = null,
    ) {}
}
