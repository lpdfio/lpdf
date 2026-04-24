import type { PageScope } from './_shared';

// ── camelCase → kebab-case helper ─────────────────────────────────────────────

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

// ── Options interfaces ────────────────────────────────────────────────────────

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

export interface RegionOptions {
  page?:  PageScope | string;
  w?:     string;
  debug?: string;
}

// ── Output node interfaces ────────────────────────────────────────────────────

export interface LpdfSpanNode {
  type:  'span';
  attrs: Record<string, string>;
  nodes: string[];
}

export interface LpdfContainerNode {
  type:  'stack' | 'flank' | 'split' | 'cluster' | 'grid' | 'frame' | 'link';
  attrs: Record<string, string>;
  nodes: LpdfNode[];
}

export interface LpdfTextNode {
  type:  'text';
  attrs: Record<string, string>;
  nodes: (string | LpdfSpanNode)[];
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
  type:  'thead';
  attrs: Record<string, string>;
  nodes: LpdfTdNode[];
}

export interface LpdfTrNode {
  type:  'tr';
  attrs: Record<string, string>;
  nodes: LpdfTdNode[];
}

export interface LpdfTdNode {
  type:  'td';
  attrs: Record<string, string>;
  nodes: LpdfNode[];
}

export interface LpdfTableNode {
  type:  'table';
  attrs: Record<string, string>;
  nodes: (LpdfTheadNode | LpdfTrNode)[];
}

export interface LpdfRegionNode {
  type:  'layout-region';
  attrs: Record<string, string>;
  nodes: LpdfNode[];
}

export type LpdfNode =
  | LpdfContainerNode
  | LpdfTextNode
  | LpdfDividerNode
  | LpdfTableNode
  | LpdfImgNode
  | LpdfBarcodeNode
  | LpdfRegionNode;

// ── Helper ────────────────────────────────────────────────────────────────────

function makeContainer(
  type: LpdfContainerNode['type'],
  nodes: LpdfNode[],
  options?: Record<string, string | undefined>,
): LpdfContainerNode {
  return { type, attrs: buildAttrs(options ?? {}), nodes };
}

// ── LpdfLayout factory ────────────────────────────────────────────────────────

function stack(nodes: LpdfNode[] = [], options?: StackOptions): LpdfContainerNode {
  return makeContainer('stack', nodes, options as Record<string, string | undefined>);
}

function flank(nodes: LpdfNode[] = [], options?: FlankOptions): LpdfContainerNode {
  return makeContainer('flank', nodes, options as Record<string, string | undefined>);
}

function split(nodes: LpdfNode[] = [], options?: SplitOptions): LpdfContainerNode {
  return makeContainer('split', nodes, options as Record<string, string | undefined>);
}

function cluster(nodes: LpdfNode[] = [], options?: ClusterOptions): LpdfContainerNode {
  return makeContainer('cluster', nodes, options as Record<string, string | undefined>);
}

function grid(nodes: LpdfNode[] = [], options?: GridOptions): LpdfContainerNode {
  return makeContainer('grid', nodes, options as Record<string, string | undefined>);
}

function frame(nodes: LpdfNode[] = [], options?: FrameOptions): LpdfContainerNode {
  return makeContainer('frame', nodes, options as Record<string, string | undefined>);
}

function link(nodes: LpdfNode[] = [], options?: LinkOptions): LpdfContainerNode {
  return makeContainer('link', nodes, options as Record<string, string | undefined>);
}

function table(options: TableOptions, nodes: (LpdfTheadNode | LpdfTrNode)[] = []): LpdfTableNode {
  return {
    type:  'table',
    attrs: buildAttrs(options as unknown as Record<string, string | undefined>),
    nodes,
  };
}

function thead(nodes: LpdfTdNode[] = [], options?: TheadOptions): LpdfTheadNode {
  return {
    type:  'thead',
    attrs: buildAttrs((options ?? {}) as Record<string, string | undefined>),
    nodes,
  };
}

function tr(nodes: LpdfTdNode[] = [], options?: TrOptions): LpdfTrNode {
  return {
    type:  'tr',
    attrs: buildAttrs((options ?? {}) as Record<string, string | undefined>),
    nodes,
  };
}

function td(nodes: LpdfNode[] = [], options?: TdOptions): LpdfTdNode {
  return {
    type:  'td',
    attrs: buildAttrs((options ?? {}) as Record<string, string | undefined>),
    nodes,
  };
}

function text(nodes: (string | LpdfSpanNode)[] = [], options?: TextOptions): LpdfTextNode {
  return {
    type:  'text',
    attrs: buildAttrs((options ?? {}) as Record<string, string | undefined>),
    nodes,
  };
}

function span(nodes: string[] = [], options?: SpanOptions): LpdfSpanNode {
  return {
    type:  'span',
    attrs: buildAttrs((options ?? {}) as Record<string, string | undefined>),
    nodes,
  };
}

function divider(options?: DividerOptions): LpdfDividerNode {
  return {
    type:  'divider',
    attrs: buildAttrs((options ?? {}) as Record<string, string | undefined>),
  };
}

function img(options: ImgOptions): LpdfImgNode {
  return {
    type:  'img',
    attrs: buildAttrs(options as unknown as Record<string, string | undefined>),
  };
}

function barcode(options: BarcodeOptions): LpdfBarcodeNode {
  return {
    type:  'barcode',
    attrs: buildAttrs(options as unknown as Record<string, string | undefined>),
  };
}

function region(pin: string, nodes: LpdfNode[] = [], options?: RegionOptions): LpdfRegionNode {
  const attrs: Record<string, string> = { pin };
  if (options?.page  !== undefined) attrs['page']  = options.page;
  if (options?.w     !== undefined) attrs['w']     = options.w;
  if (options?.debug !== undefined) attrs['debug'] = options.debug;
  return { type: 'layout-region', attrs, nodes };
}

export const LpdfLayout = Object.freeze({
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
  table,
  thead,
  tr,
  td,
  region,
});
