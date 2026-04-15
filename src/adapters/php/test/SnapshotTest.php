<?php

declare(strict_types=1);

namespace Lpdf\Tests;

use Lpdf\LpdfEngine;
use PHPUnit\Framework\TestCase;

final class SnapshotTest extends TestCase
{
    private static string $root;
    private static string $fixtures;
    private static string $snapshots;

    public static function setUpBeforeClass(): void
    {
        self::$root      = self::findRoot();
        self::$fixtures  = self::$root . '/test/fixtures';
        self::$snapshots = self::$root . '/test/snapshots';
    }

    /** @return array<string, array{string}> */
    public static function fixtureProvider(): array
    {
        $names = [];
        for ($i = 1; $i <= 11; $i++) {
            $names["example$i"] = ["example$i"];
        }
        return $names;
    }

    #[\PHPUnit\Framework\Attributes\DataProvider('fixtureProvider')]
    public function testFixtureMatchesStoredHash(string $name): void
    {
        $xml    = file_get_contents(self::$fixtures . "/$name.xml");
        $engine = new LpdfEngine('test-key');
        $bytes  = $engine->renderPdf($xml);
        $hash   = hash('sha256', $bytes);
        $snap   = self::$snapshots . "/$name.pdf.sha256";

        if (getenv('UPDATE_SNAPSHOTS') === '1') {
            file_put_contents($snap, $hash);
        } else {
            $stored = trim(file_get_contents($snap));
            self::assertSame($stored, $hash);
        }
    }

    public function testOutputIsPdf(): void
    {
        $xml   = file_get_contents(self::$fixtures . '/example1.xml');
        $bytes = (new LpdfEngine('test-key'))->renderPdf($xml);
        self::assertStringStartsWith('%PDF-', $bytes);
    }

    public function testCustomFontDoesNotThrow(): void
    {
        $xml    = file_get_contents(self::$fixtures . '/example1.xml');
        $engine = new LpdfEngine('test-key');

        // Load a placeholder font (empty bytes will be ignored by the core if
        // the document does not reference it; we just assert no exception).
        $engine->loadFont('TestFont', '');
        $bytes = $engine->renderPdf($xml);
        self::assertStringStartsWith('%PDF-', $bytes);
    }

    private static function findRoot(): string
    {
        $dir = \dirname(__DIR__);
        while ($dir !== \dirname($dir)) {
            if (file_exists($dir . '/composer.json')) {
                return $dir;
            }
            $dir = \dirname($dir);
        }
        throw new \RuntimeException('Could not locate project root.');
    }
}
