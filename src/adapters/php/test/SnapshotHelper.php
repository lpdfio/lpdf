<?php

declare(strict_types=1);

namespace Lpdf\Tests;

use PHPUnit\Framework\Assert;

/**
 * Shared snapshot-test helpers: path resolution, SHA-256 hashing, and
 * compare-or-update logic. Used by SnapshotTest.
 */
final class SnapshotHelper
{
    /** @return array<string, array{string}> */
    public static function fixtureProvider(): array
    {
        $names = [];
        for ($i = 1; $i <= 11; $i++) {
            $names["example$i"] = ["example$i"];
        }
        foreach (['showcase-cluster', 'showcase-flank', 'showcase-frame', 'showcase-grid', 'showcase-split', 'showcase-stack', 'showcase-table', 'showcase-barcode', 'showcase-encryption', 'showcase-forms'] as $name) {
            $names[$name] = [$name];
        }
        foreach (['bench_xs', 'bench_s', 'bench_m', 'bench_l', 'bench_xl'] as $name) {
            $names[$name] = [$name];
        }
        return $names;
    }

    /**
     * Compares the SHA-256 hash of $bytes against the stored snapshot for $name,
     * or writes a new snapshot when UPDATE_SNAPSHOTS=1.
     */
    public static function compareOrUpdate(string $name, string $bytes): void
    {
        $hash = hash('sha256', $bytes);
        $snap = self::snapshots() . "/$name.pdf.sha256";

        if (getenv('UPDATE_SNAPSHOTS') === '1' || !file_exists($snap)) {
            file_put_contents($snap, $hash);
        } else {
            $stored = trim((string) file_get_contents($snap));
            Assert::assertSame($stored, $hash);
        }
    }

    /** Returns the absolute path to the shared test/fixtures directory. */
    public static function fixtures(): string
    {
        static $path = null;
        $path ??= self::findRoot() . '/test/fixtures';
        return $path;
    }

    /** Returns the absolute path to the shared test/snapshots directory. */
    public static function snapshots(): string
    {
        static $path = null;
        $path ??= self::findRoot() . '/test/snapshots';
        return $path;
    }

    private static function findRoot(): string
    {
        static $root = null;
        if ($root !== null) {
            return $root;
        }
        $dir = \dirname(__DIR__);
        while ($dir !== \dirname($dir)) {
            // Accept a directory that has Cargo.toml (native) OR test/snapshots (Docker).
            if (file_exists($dir . '/Cargo.toml') || is_dir($dir . '/test/snapshots')) {
                $root = $dir;
                return $root;
            }
            $dir = \dirname($dir);
        }
        throw new \RuntimeException('Could not locate project root (Cargo.toml not found).');
    }
}
