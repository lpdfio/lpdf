<?php

declare(strict_types=1);

namespace Lpdf;

use Lpdf\Kit\Document;
use Lpdf\Kit\DocumentOptions;
use Lpdf\Kit\SectionCanvas;
use Lpdf\Kit\SectionLayout;
use Lpdf\Kit\SectionNode;
use Lpdf\Kit\SectionOptions;
use Lpdf\Canvas\LayerNode;
use Lpdf\Shared\AttrsHelper;
use Lpdf\Layout\Node;

/**
 * Static factory for assembling the document skeleton.
 *
 * `Kit` puts together the parts built by {@see Layout} and {@see Canvas}.
 * Pass the result of {@see document()} to {@see Engine::renderPdf()}.
 *
 * @example
 * ```php
 * use Lpdf\Kit;
 * use Lpdf\Kit\{DocumentOptions, SectionOptions, DocumentMeta, DocumentTokens};
 * use Lpdf\Layout;
 * use Lpdf\Canvas;
 *
 * $doc = Kit::document(
 *     sections: [
 *         Kit::section(
 *             nodes: [
 *                 Kit::canvas([ Canvas::layer([ Canvas::rect(0, 0, 595, 842) ]) ]),
 *                 Kit::layout([ Layout::text(['Hello']) ]),
 *             ],
 *             options: new SectionOptions(size: 'a4', margin: '28pt'),
 *         ),
 *     ],
 *     options: new DocumentOptions(meta: new DocumentMeta(title: 'My Doc')),
 * );
 * $pdf = (new Engine('license-key'))->renderPdf($doc);
 * ```
 */
final class LpdfKit
{
    use AttrsHelper;
    /**
     * Build a layout block for use inside {@see section()}.
     *
     * @param Node[] $nodes
     */
    public static function layout(array $nodes = []): SectionLayout
    {
        return new SectionLayout($nodes);
    }

    /**
     * Build a canvas block for use inside {@see section()}.
     *
     * @param LayerNode[] $layers Layers produced by {@see Canvas::layer()}.
     */
    public static function canvas(array $layers = []): SectionCanvas
    {
        return new SectionCanvas($layers);
    }

    /**
     * Build a section node containing an ordered sequence of layout and/or canvas blocks.
     *
     * Children are painted in order — first child is at the bottom, last is on top.
     *
     * ```php
     * // Canvas underlay (canvas behind layout):
     * Kit::section(nodes: [
     *     Kit::canvas([ Canvas::layer([...]) ]),
     *     Kit::layout([ Layout::text(['Hello']) ]),
     * ])
     * ```
     *
     * @param array<SectionLayout|SectionCanvas> $nodes  In paint order.
     */
    public static function section(
        array            $nodes   = [],
        ?SectionOptions  $options = null,
    ): SectionNode {
        return new SectionNode(self::optionsToAttrs($options), $nodes);
    }

    /**
     * Build the root document node, ready for {@see Engine::renderPdf()}.
     *
     * @param SectionNode[] $sections
     */
    public static function document(array $sections = [], ?DocumentOptions $options = null): Document
    {
        // tokens and meta are sub-objects, not flat string attrs — handle separately.
        $attrs = self::optionsToAttrs($options, skip: ['tokens', 'meta']);

        if ($options?->tokens !== null) {
            $attrs['tokens'] = $options->tokens;
        }
        if ($options?->meta !== null) {
            $attrs['meta'] = $options->meta;
        }

        return new Document($attrs, $sections);
    }

}
