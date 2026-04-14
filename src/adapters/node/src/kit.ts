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
  width?:      string;
  height?:     string;
  border?:     string;
  radius?:     string;
}

export interface FlankOptions {
  gap?:        string;
  padding?:    string;
  background?: string;
  align?:      string;
  justify?:    string;
  wrap?:       string;
  width?:      string;
  height?:     string;
  border?:     string;
  radius?:     string;
}

export interface SplitOptions {
  gap?:        string;
  padding?:    string;
  background?: string;
  width?:      string;
  height?:     string;
  border?:     string;
  radius?:     string;
}

export interface ClusterOptions {
  gap?:        string;
  padding?:    string;
  background?: string;
  align?:      string;
  width?:      string;
  height?:     string;
  border?:     string;
  radius?:     string;
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
}

export interface FrameOptions {
  width?:      string;
  height?:     string;
  padding?:    string;
  background?: string;
  border?:     string;
  radius?:     string;
  align?:      string;
}

export interface LinkOptions {
  url?:  string;
  width?: string;
  height?: string;
}

export interface TextOptions {
  font?:       string;
  fontSize?:   string;
  textAlign?:  string;
  color?:      string;
  bold?:       string;
  padding?:    string;
  background?: string;
  repeat?:     string;
}

export interface SpanOptions {
  font?:      string;
  fontSize?:  string;
  color?:     string;
  bold?:      string;
  url?:       string;
}

export interface DividerOptions {
  color?:      string;
  thickness?:  string;
  direction?:  string;
}

export interface PageOptions {
  size?:        string;
  orientation?: string;
  margin?:      string;
  background?:  string;
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
export interface PageInput     { nodes?: LpdfNode[];                  options?: PageOptions;     }
export interface DocumentInput { nodes?: LpdfPageNode[];              options?: DocumentOptions; }

// ── Output node shapes (what helpers return / the serialised tree) ─────────────

/** Spans are only valid as children of `LpdfTextNode` — not part of `LpdfNode`. */
export interface LpdfSpanNode {
  type:     'span';
  attrs:    Record<string, string>;
  children: string[];
}

export interface LpdfContainerNode {
  type:     'stack' | 'flank' | 'split' | 'cluster' | 'grid' | 'frame' | 'link';
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

export type LpdfNode = LpdfContainerNode | LpdfTextNode | LpdfDividerNode;

export interface LpdfPageNode {
  type:     'page';
  attrs:    Record<string, string>;
  children: LpdfNode[];
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

function page(input: PageInput = {}): LpdfPageNode {
  return {
    type:     'page',
    attrs:    buildAttrs((input.options ?? {}) as Record<string, string | undefined>),
    children: input.nodes ?? [],
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
  page,
  document,
});
