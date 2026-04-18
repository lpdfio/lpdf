export declare class LpdfRenderError extends Error {
    constructor(message: string);
}
export declare function renderPdf(xml: string, licenseKey: string, xmlDir: string): Promise<Uint8Array>;
export declare function disposeRenderWorker(): void;
