<?php

declare(strict_types=1);

require_once __DIR__ . '/../../../../vendor/autoload.php';

/**
 * encrypt-permissions-only.php — render showcase-encryption.xml with RC4-128 encryption,
 * no open password, print and copy disabled.
 *
 * Run (after 'make build-adapter-php'):
 *   docker run --rm \
 *     -v "$(pwd)/src/adapters/php/example:/app/src/adapters/php/example" \
 *     -v "$(pwd)/example:/app/example" \
 *     -v "$(pwd)/docs:/app/docs" \
 *     -w /app lpdf-php php src/adapters/php/example/encrypt-permissions-only.php
 *
 * Output: example/result/encrypt-permissions-only-php.pdf
 */

use Lpdf\LpdfEngine;

$root    = __DIR__ . '/../../../../example/';
$xmlFile = __DIR__ . '/../../../../docs/examples/showcase-encryption.xml';
$outputFile = 'encrypt-permissions-only-php.pdf';

$xml = file_get_contents($xmlFile);

$engine = new LpdfEngine('');  // empty key → free tier (watermark)

// Permissions only — no open password.
// File opens freely; cooperative viewers enforce print: false, copy: false.
$engine->setEncryption('', 's3cr3t', ['print' => false, 'copy' => false]);

$pdf = $engine->renderPdf($xml);

file_put_contents($root . "result/{$outputFile}", $pdf);
echo "output: $outputFile (" . number_format(strlen($pdf)) . " bytes)\n";
