import { useEffect, useRef, useState } from "react";
import { uploadWallImage, WALL_IMAGE_ACCEPT } from "../../services/loomApi/wallImages.ts";
import type { WallSourceOption } from "../../services/loomApi/wallTypes.ts";

export function WallImageImport({ baseUrl, onImported }: {
  baseUrl: string; onImported: (source: WallSourceOption) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  const uploading = useRef(false);
  useEffect(() => () => { generation.current++; }, [baseUrl]);
  async function upload(file: File) {
    if (uploading.current) return;
    uploading.current = true; setBusy(true); setError("");
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
  return <div>
    <label><span>{busy ? "正在导入图片..." : "导入图片（最多 16 MiB）"}</span>
      <input type="file" accept={WALL_IMAGE_ACCEPT} disabled={busy} onChange={(event) => {
        const file = event.target.files?.[0]; event.target.value = "";
        if (file) void upload(file);
      }} /></label>
    <p className="wall-muted">导入后点击“放置到墙面”，再保存布局。GIF 使用首帧；终端最多保留 16 张图片、合计 1677 万像素。</p>
    {error && <p role="alert" className="wall-error">{error}</p>}
  </div>;
}
