<?php

declare(strict_types=1);

namespace Lpdf\Layout;

enum FieldType: string
{
    case Text     = 'text';
    case Checkbox = 'checkbox';
    case Dropdown = 'dropdown';
    case Radio    = 'radio';
    case Button   = 'button';
}
