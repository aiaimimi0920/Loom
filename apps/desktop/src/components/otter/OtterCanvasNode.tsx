import { useEffect, useState } from "react";

import {
  getOtterCanvasNodePresentation,
  otterCanvasPreviewPath,
  type OtterCanvasLayoutNode,
} from "../../services/otterCanvas.ts";
import { loadOtterCanvasPreview } from "../../services/loomApi.ts";

interface OtterCanvasNodeProps {
  node: OtterCanvasLayoutNode;
  baseUrl: string;
  viewportWidth: number;
  viewportHeight: number;
  selected: boolean;
  interactive: boolean;
  onSelect?: (nodeId: string) => void;
}

export function OtterCanvasNode({
  node,
  baseUrl,
  viewportWidth,
  viewportHeight,
  selected,
  interactive,
  onSelect,
}: OtterCanvasNodeProps) {
  const [previewFailed, setPreviewFailed] = useState(false);
  const [previewSrc, setPreviewSrc] = useState<string | null>(null);
  const previewPath = otterCanvasPreviewPath(node);

  useEffect(() => {
    setPreviewFailed(false);
    setPreviewSrc(null);
    if (!previewPath) {
      return;
    }
    let cancelled = false;
    const controller = new AbortController();
    void loadOtterCanvasPreview(baseUrl, previewPath, controller.signal)
      .then((src) => {
        if (!cancelled) setPreviewSrc(src);
      })
      .catch(() => {
        if (!cancelled) setPreviewFailed(true);
      });
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [baseUrl, previewPath]);

  const className = [
    "otter-canvas-node",
    `otter-canvas-node--${node.kind}`,
    node.status === "error" ? "otter-canvas-node--status-error" : "",
    selected ? "otter-canvas-node--selected" : "",
  ].filter(Boolean).join(" ");

  // When Otter shows a sticker minified, the node box is a fixed unit.w×unit.h
  // window (overflow hidden) and the image is laid out at its full savedRect
  // pixel size, shifted by -cropOffset — so the box shows a 1:1 crop window of
  // the image, NOT the whole image scaled down. The daemon pre-computes ratios
  // relative to the node box, so here we just size/position the image in
  // box-relative percentages (left/top/width against the box as containing
  // block), exactly mirroring Otter's `img { width: savedRect.w px; left: -offset px }`.
  const renderImage = () => {
    const presentation = getOtterCanvasNodePresentation(node, {
      hasResolvedPreview: Boolean(previewSrc),
      previewFailed,
    });
    if (!presentation.showPreviewImage) {
      const placeholderClassName = [
        "otter-canvas-node__placeholder",
        presentation.placeholderTone === "error" ? "otter-canvas-node__placeholder--error" : "",
      ].filter(Boolean).join(" ");
      return (
        <span className={placeholderClassName}>
          <span className="otter-canvas-node__placeholder-title">{presentation.placeholderText}</span>
          {presentation.detailText ? (
            <span className="otter-canvas-node__placeholder-detail">{presentation.detailText}</span>
          ) : null}
        </span>
      );
    }
    if (!previewSrc) {
      return <span className="otter-canvas-node__placeholder">预览不可用</span>;
    }
    if (
      node.minified
      && node.crop
      && node.crop.imageWidthRatio > 0
      && node.crop.imageHeightRatio > 0
    ) {
      return (
        <img
          className="otter-canvas-node__crop"
          src={previewSrc}
          alt=""
          aria-hidden="true"
          draggable={false}
          onError={() => setPreviewFailed(true)}
          style={{
            position: "absolute",
            left: `${-node.crop.offsetXRatio * 100}%`,
            top: `${-node.crop.offsetYRatio * 100}%`,
            width: `${node.crop.imageWidthRatio * 100}%`,
            height: `${node.crop.imageHeightRatio * 100}%`,
            maxWidth: "none",
            maxHeight: "none",
            objectFit: "fill",
            display: "block",
            opacity: node.opacity,
          }}
        />
      );
    }
    return (
      <img
        src={previewSrc}
        alt=""
        aria-hidden="true"
        draggable={false}
        onError={() => setPreviewFailed(true)}
        style={{ opacity: node.opacity }}
      />
    );
  };

  return (
    <button
      className={className}
      data-testid="otter-canvas-node"
      data-node-id={node.id}
      type="button"
      disabled={!interactive}
      aria-label={node.label}
      aria-pressed={selected}
      onClick={() => onSelect?.(node.id)}
      style={{
        left: `${(node.x / viewportWidth) * 100}%`,
        top: `${(node.y / viewportHeight) * 100}%`,
        width: `${(node.width / viewportWidth) * 100}%`,
        height: `${(node.height / viewportHeight) * 100}%`,
      }}
    >
      {renderImage()}
    </button>
  );
}
