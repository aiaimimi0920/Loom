// Adapts the Otter canvas thumbnail to the desktop workspace.
import { type OtterCanvasSnapshot } from "../../services/otterCanvas";
import { LoomToolDefinition } from "../../services/loomApi";
import { OtterCanvasThumbnail, type WorkflowArtCreationRequest } from "./OtterCanvasThumbnail";

export function OtterBridgePanel({
  baseUrl,
  otterCanvas,
  otterCanvasError,
  tools,
  onCreateWorkflowArt,
}: {
  baseUrl: string;
  otterCanvas: OtterCanvasSnapshot | null;
  otterCanvasError: string | null;
  tools: LoomToolDefinition[];
  onCreateWorkflowArt: (request: WorkflowArtCreationRequest) => void;
}) {
  return (
    <OtterCanvasThumbnail
      snapshot={otterCanvas}
      baseUrl={baseUrl}
      error={otterCanvasError}
      tools={tools}
      onCreateWorkflowArt={onCreateWorkflowArt}
    />
  );
}
