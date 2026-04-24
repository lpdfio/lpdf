export * from './engine';
export * from './kit';
export * from './layout';
export * from './canvas';
export * from './_shared';
export { kitToXml } from './kit-to-xml';
import { LpdfEngine } from './engine';
export declare const Lpdf: Readonly<{
    Engine: typeof LpdfEngine;
    Kit: Readonly<{
        layout: (nodes?: import("./layout").LpdfNode[]) => import("./kit").LpdfLayoutBlock;
        canvas: (layers?: import("./canvas").LpdfCanvasLayerNode[]) => import("./kit").LpdfCanvasBlock;
        section: (input?: import("./kit").SectionInput) => import("./kit").LpdfSectionNode;
        document: (input?: import("./kit").DocumentInput) => import("./kit").LpdfDocument;
    }>;
    Layout: Readonly<{
        stack: (nodes?: import("./layout").LpdfNode[], options?: import("./layout").StackOptions) => import("./layout").LpdfContainerNode;
        flank: (nodes?: import("./layout").LpdfNode[], options?: import("./layout").FlankOptions) => import("./layout").LpdfContainerNode;
        split: (nodes?: import("./layout").LpdfNode[], options?: import("./layout").SplitOptions) => import("./layout").LpdfContainerNode;
        cluster: (nodes?: import("./layout").LpdfNode[], options?: import("./layout").ClusterOptions) => import("./layout").LpdfContainerNode;
        grid: (nodes?: import("./layout").LpdfNode[], options?: import("./layout").GridOptions) => import("./layout").LpdfContainerNode;
        frame: (nodes?: import("./layout").LpdfNode[], options?: import("./layout").FrameOptions) => import("./layout").LpdfContainerNode;
        link: (nodes?: import("./layout").LpdfNode[], options?: import("./layout").LinkOptions) => import("./layout").LpdfContainerNode;
        text: (nodes?: (string | import("./layout").LpdfSpanNode)[], options?: import("./layout").TextOptions) => import("./layout").LpdfTextNode;
        span: (nodes?: string[], options?: import("./layout").SpanOptions) => import("./layout").LpdfSpanNode;
        divider: (options?: import("./layout").DividerOptions) => import("./layout").LpdfDividerNode;
        img: (options: import("./layout").ImgOptions) => import("./layout").LpdfImgNode;
        barcode: (options: import("./layout").BarcodeOptions) => import("./layout").LpdfBarcodeNode;
        table: (options: import("./layout").TableOptions, nodes?: (import("./layout").LpdfTheadNode | import("./layout").LpdfTrNode)[]) => import("./layout").LpdfTableNode;
        thead: (nodes?: import("./layout").LpdfTdNode[], options?: import("./layout").TheadOptions) => import("./layout").LpdfTheadNode;
        tr: (nodes?: import("./layout").LpdfTdNode[], options?: import("./layout").TrOptions) => import("./layout").LpdfTrNode;
        td: (nodes?: import("./layout").LpdfNode[], options?: import("./layout").TdOptions) => import("./layout").LpdfTdNode;
        region: (pin: string, nodes?: import("./layout").LpdfNode[], options?: import("./layout").RegionOptions) => import("./layout").LpdfRegionNode;
    }>;
    Canvas: Readonly<{
        rect: (x: number, y: number, w: number, h: number, style?: import("./canvas").CanvasRectStyle) => import("./canvas").LpdfCanvasRectNode;
        line: (x1: number, y1: number, x2: number, y2: number, style?: import("./canvas").CanvasLineStyle) => import("./canvas").LpdfCanvasLineNode;
        ellipse: (cx: number, cy: number, rx: number, ry: number, style?: import("./canvas").CanvasEllipseStyle) => import("./canvas").LpdfCanvasEllipseNode;
        circle: (cx: number, cy: number, r: number, style?: import("./canvas").CanvasEllipseStyle) => import("./canvas").LpdfCanvasCircleNode;
        path: (d: string, style?: import("./canvas").CanvasPathStyle) => import("./canvas").LpdfCanvasPathNode;
        text: (x: number, y: number, content: string, style?: import("./canvas").CanvasTextStyle, runs?: import("./canvas").CanvasRun[]) => import("./canvas").LpdfCanvasTextNode;
        img: (x: number, y: number, w: number, h: number, name: string) => import("./canvas").LpdfCanvasImgNode;
        layer: (nodes: import("./canvas").LpdfCanvasPrimitiveNode[], options?: import("./canvas").CanvasLayerOptions) => import("./canvas").LpdfCanvasLayerNode;
    }>;
}>;
