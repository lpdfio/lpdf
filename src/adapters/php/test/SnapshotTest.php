<?php

declare(strict_types=1);

namespace Lpdf\Tests;

use Lpdf\LpdfEngine;
use PHPUnit\Framework\TestCase;

final class SnapshotTest extends TestCase
{
    /** @return array<string, array{string}> */
    public static function fixtureProvider(): array
    {
        return SnapshotHelper::fixtureProvider();
    }

    #[\PHPUnit\Framework\Attributes\DataProvider('fixtureProvider')]
    public function testFixtureMatchesStoredHash(string $name): void
    {
        $xml    = file_get_contents(SnapshotHelper::fixtures() . "/$name.xml");
        $engine = new LpdfEngine('test-key');
        $bytes  = $engine->renderPdf($xml);
        SnapshotHelper::compareOrUpdate($name, $bytes);
    }

    public function testOutputIsPdf(): void
    {
        $xml   = file_get_contents(SnapshotHelper::fixtures() . '/example1.xml');
        $bytes = (new LpdfEngine('test-key'))->renderPdf($xml);
        self::assertStringStartsWith('%PDF-', $bytes);
    }

    public function testCustomFontDoesNotThrow(): void
    {
        $xml    = file_get_contents(SnapshotHelper::fixtures() . '/example1.xml');
        $engine = new LpdfEngine('test-key');

        // Load a placeholder font (empty bytes will be ignored by the core if
        // the document does not reference it; we just assert no exception).
        $engine->loadFont('TestFont', '');
        $bytes = $engine->renderPdf($xml);
        self::assertStringStartsWith('%PDF-', $bytes);
    }
}
