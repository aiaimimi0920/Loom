// Core native document and image interfaces; omits lib.dom so fetch, URL, and browser HTML APIs are never advertised to plugins.
import type { ArtCanvasContext } from "./native-canvas";
declare const imageIdentity: unique symbol;
export interface ArtImage {
    readonly [imageIdentity]: never;
    readonly width: number;
    readonly height: number;
    readonly naturalWidth: number;
    readonly naturalHeight: number;
}
export interface ArtEvent {
    readonly type: string;
    readonly target: ArtNode | null;
    readonly currentTarget: ArtNode | null;
    readonly isTrusted: boolean;
    readonly defaultPrevented: boolean;
    preventDefault(): void;
    stopPropagation(): void;
    stopImmediatePropagation(): void;
}
export type ArtListener = (event: ArtEvent) => void | Promise<void>;
export type ArtListenerOptions = boolean | { capture?: boolean; once?: boolean; passive?: boolean };
export interface ArtCollection<T> extends Iterable<T> {
    readonly length: number;
    readonly [index: number]: T;
    item(index: number): T | null;
}
export interface ArtNode {
    textContent: string;
    readonly parentNode: ArtNode | null;
    readonly childNodes: ArtCollection<ArtNode>;
    appendChild<T extends ArtNode>(child: T): T;
    append(...children: (ArtNode | string)[]): void;
    replaceChildren(...children: (ArtNode | string)[]): void;
    removeChild<T extends ArtNode>(child: T): T;
    remove(): void;
    contains(other: ArtNode | null): boolean;
    addEventListener(type: string, listener: ArtListener, options?: ArtListenerOptions): void;
    removeEventListener(type: string, listener: ArtListener, options?: boolean | { capture?: boolean }): void;
}
export interface ArtElement extends ArtNode {
    id: string;
    className: string;
    readonly tagName: string;
    readonly children: ArtCollection<ArtElement>;
    setAttribute(name: string, value: string): void;
    getAttribute(name: string): string | null;
    removeAttribute(name: string): void;
    hasAttribute(name: string): boolean;
}
export interface ArtButton extends ArtElement { disabled: boolean }
export interface ArtCanvas extends ArtElement {
    width: number;
    height: number;
    getContext(kind: "2d"): ArtCanvasContext;
}
export type ArtContainerTag = "div" | "section" | "article" | "header" | "footer" | "main"
    | "span" | "strong" | "em" | "i" | "b" | "p" | "h1" | "h2" | "h3";
export interface ArtDocument {
    createElement(tag: "canvas"): ArtCanvas;
    createElement(tag: "button"): ArtButton;
    createElement(tag: ArtContainerTag): ArtElement;
    createTextNode(text: string): ArtNode;
    createDocumentFragment(): ArtNode;
    getElementById(id: string): ArtElement | null;
    querySelector(selector: string): ArtElement | null;
    querySelectorAll(selector: string): readonly ArtElement[];
}
