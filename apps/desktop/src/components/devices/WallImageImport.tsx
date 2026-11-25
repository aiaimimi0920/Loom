import { useEffect, useRef, useState } from "react";
import { Badge, Button, Notice } from "@neuro/ui";
import { uploadWallImage, WALL_IMAGE_ACCEPT } from "../../services/loomApi/wallImages.ts";
import type { WallSourceOption } from "../../services/loomApi/wallTypes.ts";

export function WallImageImport({
  baseUrl,
  onImported,
}: {
  baseUrl: string;
  onImported: (source: WallSourceOption) => void;
}) {
  const filePicker = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  const uploading = useRef(false);
  useEffect(
    () => () => {
      generation.current++;
    },
    [baseUrl],
  );
  async function upload(file: File) {
    if (uploading.current) return;
    uploading.current = true;
    setBusy(true);
    setError("");
    const ticket = generation.current;
    try {
      const source = await uploadWallImage(baseUrl, file);
      if (ticket === generation.current) onImported(source);
    } catch (error) {
      if (ticket === generation.current) setError(error instanceof Error ? error.message : "图片上传失败");
    } finally {
      uploading.current = false;
      if (ticket === generation.current) setBusy(false);
    }
  }
  return (
    <div className="wall-image-import">
      <div className="wall-toolbar">
        <Button loading={busy} onClick={() => filePicker.current?.click()}>
          导入图片
        </Button>
        <Badge>最多 16 MiB</Badge>
        <input
          ref={filePicker}
          hidden
          type="file"
          accept={WALL_IMAGE_ACCEPT}
          disabled={busy}
          onChange={(event) => {
            const file = event.target.files?.[0];
            event.target.value = "";
            if (file) void upload(file);
          }}
        />
      </div>
      <Notice tone="info">GIF 使用首帧；终端最多保留 16 张图片、合计 1677 万像素。</Notice>
      {error && <Notice tone="danger">{error}</Notice>}
    </div>
  );
}
