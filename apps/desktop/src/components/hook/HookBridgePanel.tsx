// Adapts the Hook canvas thumbnail to the desktop workspace.
import { type HookCanvasSnapshot } from "../../services/hookCanvas";
import { LoomToolDefinition } from "../../services/loomApi";
import { HookCanvasThumbnail, type WorkflowArtCreationRequest } from "./HookCanvasThumbnail";

export function HookBridgePanel({
  baseUrl,
  hookCanvas,
  hookCanvasError,
  tools,
  onCreateWorkflowArt,
}: {
  baseUrl: string;
  hookCanvas: HookCanvasSnapshot | null;
  hookCanvasError: string | null;
  tools: LoomToolDefinition[];
  onCreateWorkflowArt: (request: WorkflowArtCreationRequest) => void;
}) {
  return (
    <HookCanvasThumbnail
      snapshot={hookCanvas}
      baseUrl={baseUrl}
      error={hookCanvasError}
      tools={tools}
      onCreateWorkflowArt={onCreateWorkflowArt}
    />
  );
}
