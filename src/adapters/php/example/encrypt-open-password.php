<?php

declare(strict_types=1);

require_once __DIR__ . '/../../../../vendor/autoload.php';

/**
 * encrypt-open-password.php — render showcase-encryption.xml with RC4-128 encryption,
 * open password required, copy disabled.
 *
 * Run (after 'make build-adapter-php'):
 *   docker run --rm \
 *     -v "$(pwd)/src/adapters/php/example:/app/src/adapters/php/example" \
 *     -v "$(pwd)/example:/app/example" \
 *     -v "$(pwd)/docs:/app/docs" \
 *     -w /app lpdf-php php src/adapters/php/example/encrypt-open-password.php
 *
 * Output: example/result/encrypt-open-password-php.pdf
 */

use Lpdf\LpdfEngine;

$root    = __DIR__ . '/../../../../example/';
$xmlFile = __DIR__ . '/../../../../docs/examples/showcase-encryption.xml';
$outputFile = 'encrypt-open-password-php.pdf';

$xml = file_get_contents($xmlFile);

$engine = new LpdfEngine('');  // empty key → free tier (watermark)

// With open password — viewers prompt for 'password' before displaying content.
$engine->setEncryption('password', 'owner', ['copy' => false]);

$pdf = $engine->renderPdf($xml);

file_put_contents($root . "result/{$outputFile}", $pdf);
echo "output: $outputFile (" . number_format(strlen($pdf)) . " bytes)\n";
