/**
 * LpdfKit — tree-builder helpers for constructing lpdf document trees
 * programmatically.
 *
 * All helpers return plain serialisable objects. The resulting tree is passed
 * to `LpdfEngine.renderPdf(doc)`.
 *
 * @example
 * ```ts
 * import { LpdfEngine, LpdfKit } from 'lpdf';
 *
 * const doc = LpdfKit.document({
 *   nodes: [LpdfKit.page({ nodes: [LpdfKit.text({ nodes: ['Hello'] })] })],
 *   options: { meta: { title: 'My Doc' } },
 * });
 * const bytes = await new LpdfEngine(key).renderPdf(doc);
 * ```
 */

// ── Attribute helpers ─────────────────────────────────────────────────────────

/** camelCase → kebab-case for XML attribute names */
function attrKey(camel: string): string {
  return camel.replace(/[A-Z]/g, c => '-' + c.toLowerCase());
}

function buildAttrs(options: Record<string, string | undefined>): Record<string, string> {
  const result: Record<string, string> = {};
  for (const [key, val] of Object.entries(options)) {
    if (val !== undefined) {
      result[attrKey(key)] = val;
    }
  }
  return result;
}

// ── Options interfaces (per-primitive, only valid attributes) ─────────────────

export interface StackOptions {
  gap?:        string;
  padding?:    string;
  background?: string;
  align?:      string;
  justify?:    string;
  width?:      string;
  height?:     string;
  border?:     string;
  radius?:     string;
  debug?:      string;
}

export interface FlankOptions {
  gap?:        string;
  padding?:    string;
  background?: string;
  align?:      string;
  justify?:    string;
  end?:        string;
  width?:      string;
  height?:     string;
  border?:     string;
  radius?:     string;
  debug?:      string;
}

export interface SplitOptions {
  gap?:        string;
  padding?:    string;
  background?: string;
  align?:      string;
  equal?:      string;
  width?:      string;
  height?:     string;
  border?:     string;
  radius?:     string;
  debug?:      string;
}

export interface ClusterOptions {
  gap?:        string;
  padding?:    string;
  background?: string;
  align?:      string;
  justify?:    string;
  width?:      string;
  height?:     string;
  border?:     string;
  radius?:     string;
  debug?:      string;
}

export interface GridOptions {
  cols?:       string;
  colWidth?:   string;
  gap?:        string;
  equal?:      string;
  padding?:    string;
  background?: string;
  width?:      string;
  height?:     string;
  border?:     string;
  radius?:     string;
  debug?:      string;
}

export interface TableOptions {
  cols:        string;
  border?:     string;
  stripe?:     string;
  gap?:        string;
  padding?:    string;
  background?: string;
  width?:      string;
  height?:     string;
  repeat?:     string;
  debug?:      string;
}

export interface TheadOptions {
  background?: string;
}

export interface TrOptions {
  background?: string;
}

export interface TdOptions {
  padding?:    string;
  background?: string;
  align?:      string;
  valign?:     string;
  border?:     string;
  radius?:     string;
  gap?:        string;
  debug?:      string;
}

export interface FrameOptions {
  width?:      string;
  height?:     string;
  padding?:    string;
  background?: string;
  border?:     string;
  radius?:     string;
  align?:      string;
  debug?:      string;
}

export interface LinkOptions {
  url?:    string;
  width?:  string;
  height?: string;
  debug?:  string;
}

export interface TextOptions {
  font?:       string;
  fontSize?:   string;
  textAlign?:  string;
  color?:      string;
  bold?:       string;
  end?:        string;
  padding?:    string;
  background?: string;
  width?:      string;
  height?:     string;
  border?:     string;
  radius?:     string;
  repeat?:     string;
  debug?:      string;
}

export interface SpanOptions {
  font?:       string;
  fontSize?:   string;
  color?:      string;
  bold?:       string;
  url?:        string;
  underline?:  string;
  strike?:     string;
}

export interface DividerOptions {
  color?:      string;
  thickness?:  string;
  direction?:  string;
  debug?:      string;
}

export interface ImgOptions {
  name:        string;
  height?:     string;
  width?:      string;
  font?:       string;
  fontSize?:   string;
  gap?:        string;
  padding?:    string;
  background?: string;
  border?:     string;
  radius?:     string;
  repeat?:     string;
  debug?:      string;
}

export interface BarcodeOptions {
  type:        string;
  data:        string;
  size?:       string;
  width?:      string;
  height?:     string;
  ec?:         string;
  hrt?:        string;
  color?:      string;
  background?: string;
  repeat?:     string;
  debug?:      string;
}

export interface PageOptions {
  size?:        string;
  orientation?: string;
  margin?:      string;
  background?:  string;
  debug?:       string;
}

export interface LpdfTokens {
  colors?:  Record<string, string>;
  space?:   Record<string, string>;
  grid?:    Record<string, string>;
  border?:  Record<string, string>;
  radius?:  Record<string, string>;
  width?:   Record<string, string>;
  text?:    Record<string, string>;
  fonts?:   Record<string, LpdfFontDef>;
}

export type LpdfFontDef =
  | { src: string; builtin?: never }
  | { builtin: string; src?: never };

export interface LpdfMeta {
  title?:    string;
  author?:   string;
  subject?:  string;
  keywords?: string;
  creator?:  string;
}

export interface DocumentOptions {
  size?:        string;
  orientation?: string;
  margin?:      string;
  background?:  string;
  tokens?:      LpdfTokens;
  meta?:        LpdfMeta;
  debug?:       string;
}

// ── Input interfaces (what callers pass to each helper) ───────────────────────

export interface StackInput    { nodes?: LpdfNode[];                  options?: StackOptions;    }
export interface FlankInput    { nodes?: LpdfNode[];                  options?: FlankOptions;    }
export interface SplitInput    { nodes?: LpdfNode[];                  options?: SplitOptions;    }
export interface ClusterInput  { nodes?: LpdfNode[];                  options?: ClusterOptions;  }
export interface GridInput     { nodes?: LpdfNode[];                  options?: GridOptions;     }
export interface FrameInput    { nodes?: LpdfNode[];                  options?: FrameOptions;    }
export interface LinkInput     { nodes?: LpdfNode[];                  options?: LinkOptions;     }
export interface TextInput     { nodes?: (string | LpdfSpanNode)[];   options?: TextOptions;     }
export interface SpanInput     { nodes?: string[];                    options?: SpanOptions;     }
export interface DividerInput  {                                       options?: DividerOptions;  }
export interface ImgInput     {                                       options:  ImgOptions;      }
export interface BarcodeInput {                                       options:  BarcodeOptions;  }
export interface PageInput     { nodes?: LpdfNode[];                  options?: PageOptions;     }
export interface DocumentInput { nodes?: LpdfPageNode[];              options?: DocumentOptions; }
export interface TableInput    { nodes?: (LpdfTheadNode | LpdfTrNode)[];  options: TableOptions; }
export interface TheadInput    { nodes?: LpdfTdNode[];                options?: TheadOptions;    }
export interface TrInput       { nodes?: LpdfTdNode[];                options?: TrOptions;       }
export interface TdInput       { nodes?: LpdfNode[];                  options?: TdOptions;       }

// ── Output node shapes (what helpers return / the serialised tree) ─────────────

/** Spans are only valid as children of `LpdfTextNode` — not part of `LpdfNode`. */
export interface LpdfSpanNode {
  type:     'span';
  attrs:    Record<string, string>;
  children: string[];
}

export interface LpdfContainerNode {
  type:     'stack' | 'flank' | 'split' | 'cluster' | 'grid' | 'frame' | 'link' | 'table' | 'thead' | 'tr' | 'td';
  attrs:    Record<string, string>;
  children: LpdfNode[];
}

export interface LpdfTextNode {
  type:     'text';
  attrs:    Record<string, string>;
  children: (string | LpdfSpanNode)[];
}

export interface LpdfDividerNode {
  type:  'divider';
  attrs: Record<string, string>;
}

export interface LpdfImgNode {
  type:  'img';
  attrs: Record<string, string>;
}

export interface LpdfBarcodeNode {
  type:  'barcode';
  attrs: Record<string, string>;
}

export interface LpdfTheadNode {
  type:     'thead';
  attrs:    Record<string, string>;
  children: LpdfTdNode[];
}

export interface LpdfTrNode {
  type:     'tr';
  attrs:    Record<string, string>;
  children: LpdfTdNode[];
}

export interface LpdfTdNode {
  type:     'td';
  attrs:    Record<string, string>;
  children: LpdfNode[];
}

export interface LpdfTableNode {
  type:     'table';
  attrs:    Record<string, string>;
  children: (LpdfTheadNode | LpdfTrNode)[];
}

export type LpdfNode = LpdfContainerNode | LpdfTextNode | LpdfDividerNode | LpdfTableNode | LpdfImgNode | LpdfBarcodeNode;

export interface LpdfLayoutNode {
  type:     'layout';
  attrs:    Record<string, never>;
  children: LpdfNode[];
}

export interface LpdfPageNode {
  type:     'page';
  attrs:    Record<string, string>;
  children: LpdfLayoutNode[];
}

export interface LpdfDocument {
  version:  1;
  type:     'document';
  attrs:    Record<string, unknown>;
  children: LpdfPageNode[];
}

// ── Helper implementations ────────────────────────────────────────────────────

function makeContainer(
  type: LpdfContainerNode['type'],
  input: { nodes?: LpdfNode[]; options?: Record<string, string | undefined> },
): LpdfContainerNode {
  return {
    type,
    attrs:    buildAttrs(input.options ?? {}),
    children: input.nodes ?? [],
  };
}

function stack(input: StackInput = {}): LpdfContainerNode {
  return makeContainer('stack', input as { nodes?: LpdfNode[]; options?: Record<string, string | undefined> });
}

function flank(input: FlankInput = {}): LpdfContainerNode {
  return makeContainer('flank', input as { nodes?: LpdfNode[]; options?: Record<string, string | undefined> });
}

function split(input: SplitInput = {}): LpdfContainerNode {
  return makeContainer('split', input as { nodes?: LpdfNode[]; options?: Record<string, string | undefined> });
}

function cluster(input: ClusterInput = {}): LpdfContainerNode {
  return makeContainer('cluster', input as { nodes?: LpdfNode[]; options?: Record<string, string | undefined> });
}

function grid(input: GridInput = {}): LpdfContainerNode {
  return makeContainer('grid', input as { nodes?: LpdfNode[]; options?: Record<string, string | undefined> });
}

function frame(input: FrameInput = {}): LpdfContainerNode {
  return makeContainer('frame', input as { nodes?: LpdfNode[]; options?: Record<string, string | undefined> });
}

function link(input: LinkInput = {}): LpdfContainerNode {
  return makeContainer('link', input as { nodes?: LpdfNode[]; options?: Record<string, string | undefined> });
}

function table(input: TableInput): LpdfTableNode {
  return {
    type:     'table',
    attrs:    buildAttrs(input.options as unknown as Record<string, string | undefined>),
    children: input.nodes ?? [],
  };
}

function thead(input: TheadInput = {}): LpdfTheadNode {
  return {
    type:     'thead',
    attrs:    buildAttrs((input.options ?? {}) as Record<string, string | undefined>),
    children: input.nodes ?? [],
  };
}

function tr(input: TrInput = {}): LpdfTrNode {
  return {
    type:     'tr',
    attrs:    buildAttrs((input.options ?? {}) as Record<string, string | undefined>),
    children: input.nodes ?? [],
  };
}

function td(input: TdInput = {}): LpdfTdNode {
  return {
    type:     'td',
    attrs:    buildAttrs((input.options ?? {}) as Record<string, string | undefined>),
    children: input.nodes ?? [],
  };
}

function text(input: TextInput = {}): LpdfTextNode {
  return {
    type:     'text',
    attrs:    buildAttrs((input.options ?? {}) as Record<string, string | undefined>),
    children: input.nodes ?? [],
  };
}

function span(input: SpanInput = {}): LpdfSpanNode {
  return {
    type:     'span',
    attrs:    buildAttrs((input.options ?? {}) as Record<string, string | undefined>),
    children: input.nodes ?? [],
  };
}

function divider(input: DividerInput = {}): LpdfDividerNode {
  return {
    type:  'divider',
    attrs: buildAttrs((input.options ?? {}) as Record<string, string | undefined>),
  };
}

function img(input: ImgInput): LpdfImgNode {
  return {
    type:  'img',
    attrs: buildAttrs(input.options as unknown as Record<string, string | undefined>),
  };
}

function barcode(input: BarcodeInput): LpdfBarcodeNode {
  return {
    type:  'barcode',
    attrs: buildAttrs(input.options as unknown as Record<string, string | undefined>),
  };
}

function page(input: PageInput = {}): LpdfPageNode {
  const nodes = input.nodes ?? [];
  const children: LpdfLayoutNode[] = nodes.length > 0
    ? [{ type: 'layout', attrs: {}, children: nodes }]
    : [];
  return {
    type:     'page',
    attrs:    buildAttrs((input.options ?? {}) as Record<string, string | undefined>),
    children,
  };
}

function document(input: DocumentInput = {}): LpdfDocument {
  const { tokens, meta, ...restOpts } = input.options ?? {};
  const attrs: Record<string, unknown> = {
    ...buildAttrs(restOpts as Record<string, string | undefined>),
  };
  if (tokens !== undefined) attrs['tokens'] = tokens;
  if (meta   !== undefined) attrs['meta']   = meta;
  return {
    version:  1,
    type:     'document',
    attrs,
    children: input.nodes ?? [],
  };
}

// ── LpdfKit export ────────────────────────────────────────────────────────────

/**
 * Static builder kit — plain frozen object, not a class.
 * Import alongside `LpdfEngine`:
 *
 * ```ts
 * import { LpdfEngine, LpdfKit } from 'lpdf';
 * ```
 */
export const LpdfKit = Object.freeze({
  stack,
  flank,
  split,
  cluster,
  grid,
  frame,
  link,
  text,
  span,
  divider,
  img,
  barcode,
  page,
  document,
  table,
  thead,
  tr,
  td,
});
