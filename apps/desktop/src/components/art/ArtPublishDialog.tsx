// Publishing policy stays in Loom; the standard dialog owns all modal behavior.
import { Badge, Button, Dialog, EmptyState, Notice, Panel } from "@neuro/ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { isLocallyAuthoredTool } from "../../services/artHubUi";
import { publishArt, type LoomToolDefinition } from "../../services/loomApi";

export function authoredArtVersion(tool: LoomToolDefinition): string {
  const metadata =
    typeof tool.metadata === "object" && tool.metadata !== null && !Array.isArray(tool.metadata)
      ? (tool.metadata as Record<string, unknown>)
      : null;
  const packageSecurity =
    typeof metadata?.packageSecurity === "object" &&
    metadata.packageSecurity !== null &&
    !Array.isArray(metadata.packageSecurity)
      ? (metadata.packageSecurity as Record<string, unknown>)
      : null;
  return typeof packageSecurity?.version === "string" && packageSecurity.version.trim()
    ? packageSecurity.version.trim()
    : "0.1.0";
}

export function ArtPublishDialog({
  open,
  tools,
  baseUrl,
  onClose,
  onPublished,
}: {
  open: boolean;
  tools: LoomToolDefinition[];
  baseUrl: string;
  onClose: () => void;
  onPublished: () => Promise<void>;
}) {
  const [busyArtId, setBusyArtId] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const authoredTools = useMemo(() => tools.filter(isLocallyAuthoredTool), [tools]);
  const messageVersion = useRef(0);
  const publish = async (tool: LoomToolDefinition) => {
    const version = messageVersion.current;
    setBusyArtId(tool.id);
    setMessage(null);
    try {
      const result = await publishArt(baseUrl, tool.id);
      await onPublished();
      if (version === messageVersion.current)
        setMessage({ ok: true, text: `已发布 ${tool.name || tool.id} · ${result.globalId}` });
    } catch (error) {
      if (version === messageVersion.current)
        setMessage({ ok: false, text: error instanceof Error ? error.message : "发布失败。" });
    } finally {
      setBusyArtId(null);
    }
  };
  useEffect(() => {
    if (!open) setMessage(null);
    // External creation requests may replace a busy dialog; its late receipt is not part of the next session.
    return () => {
      messageVersion.current += 1;
    };
  }, [open]);
  if (!open) return null;
  return (
    <Dialog
      open
      title="发布 Art"
      closeLabel="关闭发布 Art"
      className="loom-art-publish-dialog"
      busy={busyArtId !== null}
      onOpenChange={(value) => {
        if (!value) onClose();
      }}
    >
      <div className="loom-art-publish-dialog__content">
        {message ? (
          <Notice tone={message.ok ? "success" : "danger"} role={message.ok ? "status" : "alert"}>
            {message.text}
          </Notice>
        ) : null}
        <div className="art-publish-dialog__list">
          {authoredTools.map((tool) => (
            <Panel className="art-publish-dialog__row" key={tool.id}>
              <div className="art-publish-dialog__identity">
                <h3 title={tool.name || tool.id}>{tool.name || tool.id}</h3>
                <p className="mono-line" title={tool.id}>
                  {tool.id}
                </p>
              </div>
              <Badge className="art-publish-dialog__version" title={authoredArtVersion(tool)}>
                <span>{authoredArtVersion(tool)}</span>
              </Badge>
              <Button
                variant="primary"
                loading={busyArtId === tool.id}
                disabled={busyArtId !== null}
                onClick={() => void publish(tool)}
              >
                发布
              </Button>
            </Panel>
          ))}
          {authoredTools.length === 0 ? <EmptyState title="暂无本地创建的 Art" /> : null}
        </div>
      </div>
    </Dialog>
  );
}
