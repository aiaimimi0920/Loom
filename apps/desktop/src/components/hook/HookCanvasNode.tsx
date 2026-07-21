import { useEffect, useState } from "react";

import {
  resolveHookCanvasPreviewUrl,
  type HookCanvasLayoutNode,
} from "../../services/hookCanvas.ts";

interface HookCanvasNodeProps {
  node: HookCanvasLayoutNode;
  baseUrl: string;
  viewportWidth: number;
  viewportHeight: number;
  selected: boolean;
  interactive: boolean;
  onSelect?: (nodeId: string) => void;
}

const statusLabel: Record<HookCanvasLayoutNode["status"], string> = {
  ready: "就绪",
  processing: "处理中",
  error: "异常",
  unknown: "未知",
};

export function HookCanvasNode({
  node,
  baseUrl,
  viewportWidth,
  viewportHeight,
  selected,
  interactive,
  onSelect,
}: HookCanvasNodeProps) {
  const [previewFailed, setPreviewFailed] = useState(false);
  const previewUrl = resolveHookCanvasPreviewUrl(baseUrl, node);

  useEffect(() => {
    setPreviewFailed(false);
  }, [previewUrl]);

  const className = [
    "hook-canvas-node",
    `hook-canvas-node--${node.kind}`,
    selected ? "hook-canvas-node--selected" : "",
  ].filter(Boolean).join(" ");

  return (
    <button
      className={className}
      data-testid="hook-canvas-node"
      data-node-id={node.id}
      type="button"
      disabled={!interactive}
      aria-label={node.label}
      onClick={() => onSelect?.(node.id)}
      style={{
        left: `${(node.x / viewportWidth) * 100}%`,
        top: `${(node.y / viewportHeight) * 100}%`,
        width: `${(node.width / viewportWidth) * 100}%`,
        height: `${(node.height / viewportHeight) * 100}%`,
      }}
    >
      {previewUrl && !previewFailed ? (
        <img
          src={previewUrl}
          alt=""
          aria-hidden="true"
          onError={() => setPreviewFailed(true)}
        />
      ) : (
        <span className="hook-canvas-node__placeholder">预览不可用</span>
      )}
      <span className="hook-canvas-node__label">
        <strong>{node.label}</strong>
        <small>{statusLabel[node.status]}</small>
      </span>
    </button>
  );
}
