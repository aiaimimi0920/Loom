// Declares only the supported native Canvas 2D subset; drawImage accepts exclusively host-installed unforgeable ArtImage handles.
import type { ArtCanvas, ArtImage } from "./native-dom";
export interface ArtCanvasContext {
    readonly canvas: ArtCanvas;
    fillStyle: string;
    strokeStyle: string;
    globalAlpha: number;
    imageSmoothingEnabled: boolean;
    lineWidth: number;
    lineCap: "butt" | "round" | "square";
    lineJoin: "miter" | "round" | "bevel";
    miterLimit: number;
    lineDashOffset: number;
    font: string;
    textAlign: "left" | "right" | "center" | "start" | "end";
    textBaseline: "alphabetic" | "top" | "middle" | "bottom";
    save(): void;
    restore(): void;
    setTransform(a: number, b: number, c: number, d: number, e: number, f: number): void;
    resetTransform(): void;
    clearRect(x: number, y: number, width: number, height: number): void;
    fillRect(x: number, y: number, width: number, height: number): void;
    strokeRect(x: number, y: number, width: number, height: number): void;
    drawImage(image: ArtImage, x: number, y: number): void;
    drawImage(image: ArtImage, x: number, y: number, width: number, height: number): void;
    drawImage(image: ArtImage, sx: number, sy: number, sw: number, sh: number,
        dx: number, dy: number, dw: number, dh: number): void;
    beginPath(): void;
    closePath(): void;
    moveTo(x: number, y: number): void;
    lineTo(x: number, y: number): void;
    rect(x: number, y: number, width: number, height: number): void;
    bezierCurveTo(cp1x: number, cp1y: number, cp2x: number, cp2y: number, x: number, y: number): void;
    arc(x: number, y: number, radius: number, start: number, end: number, anticlockwise?: boolean): void;
    fill(rule?: "nonzero" | "evenodd"): void;
    stroke(): void;
    fillText(text: string, x: number, y: number): void;
    setLineDash(segments: number[]): void;
    getLineDash(): number[];
}
