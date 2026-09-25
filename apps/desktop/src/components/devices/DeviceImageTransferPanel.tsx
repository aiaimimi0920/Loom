import { useEffect, useRef, useState } from "react";
import { readWallState } from "../../services/loomApi/walls.ts";
import { listManagedDevices } from "../../services/loomApi/hook.ts";
import { deviceImageDelivery, imageTarget, sendDeviceImage, type DeviceImageReceipt } from "../../services/loomApi/deviceImageTransfer.ts";
import { WALL_IMAGE_ACCEPT } from "../../services/loomApi/wallImages.ts";
import type { WallState } from "../../services/loomApi/wallTypes.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";

export function DeviceImageTransferPanel({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const [state, setState] = useState<WallState | null>(null);
  const [names, setNames] = useState<Record<string, string>>({});
  const [endpointId, setEndpointId] = useState("");
  const [file, setFile] = useState<File | null>(null);
  const [error, setError] = useState("");
  const [pollError, setPollError] = useState("");
  const [busy, setBusy] = useState(false);
  const [receipt, setReceipt] = useState<DeviceImageReceipt | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const pending = useRef<AbortController | null>(null);
  useEffect(() => {
    setState(null); setReceipt(null); setError(""); setEndpointId(""); setFile(null);
    return () => { pending.current?.abort(); };
  }, [baseUrl, online]);
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    async function poll() {
      try {
        const [next, devices] = await Promise.all([readWallState(baseUrl), listManagedDevices(baseUrl)]);
        if (!disposed) {
          setState((current) => current && current.revision > next.revision ? current : next);
          setNames(Object.fromEntries(devices.devices.map((device) => [device.id, device.name])));
          setPollError("");
        }
      } catch (error) { if (!disposed) setPollError(errorMessage(error)); }
      finally { if (!disposed) timer = setTimeout(() => void poll(), 2000); }
    }
    if (online) void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, [baseUrl, online, refreshKey]);
  async function send() {
    if (!file || !endpointId || !online || pending.current) return;
    const controller = new AbortController();
    pending.current = controller; setBusy(true); setError(""); setReceipt(null);
    try {
      const result = await sendDeviceImage(baseUrl, endpointId, file, controller.signal);
      if (!controller.signal.aborted) { setReceipt(result); setRefreshKey((key) => key + 1); }
    } catch (error) {
      if (!controller.signal.aborted) setError(errorMessage(error) + "。若提交时网络中断，请先核对屏幕墙状态，勿盲目重发。");
    } finally {
      if (pending.current === controller) pending.current = null;
      if (!controller.signal.aborted) setBusy(false);
    }
  }
  let targetError = "";
  if (state && endpointId) {
    try { imageTarget(state, endpointId); } catch (error) { targetError = errorMessage(error); }
  }
  return <section className="wall-management" aria-label="发送图片到设备">
    <header><h2>发送图片</h2>
      <p className="wall-muted">向此 Loom 已批准的终端发送一张图片，无需中心账号登录。接收端需运行 Hook 终端并选择输出屏幕。</p>
      <p className="wall-muted">图片由此 Loom 传给目标终端。后续发送会替换该输出的上一张图片；需要持续同步贴图时使用二维码投射。</p>
    </header>
    {!online && <p role="status" className="wall-warning">本机 Loom 离线。</p>}
    {online && !state && !pollError && <p role="status">正在读取终端...</p>}
    {state && !state.endpoints.length && <p role="status">尚无接收终端。请让 PC3 的 Hook 连接此 Loom，批准配对后，在 PC3 打开终端并注册屏幕。</p>}
    <label className="wall-toolbar"><span>接收设备 / 屏幕</span>
      <select className="studio-input" value={endpointId} disabled={busy || !online || !state?.endpoints.length}
        onChange={(event) => { setEndpointId(event.target.value); setError(""); }}>
        <option value="">选择接收终端</option>
        {state?.endpoints.map(({ endpoint, online }) => <option key={endpoint.endpointId} value={endpoint.endpointId}>
          {names[endpoint.deviceId] ?? endpoint.deviceId} / {endpoint.display?.name ?? endpoint.outputId} · {online ? "在线" : "离线"}
        </option>)}
      </select>
    </label>
    <label className="wall-toolbar"><span>图片（最多 16 MiB）</span>
      <input type="file" accept={WALL_IMAGE_ACCEPT} disabled={busy || !online}
        onChange={(event) => { setFile(event.target.files?.[0] ?? null); setError(""); }} />
    </label>
    {file && <p className="wall-muted">{file.name} · {Math.ceil(file.size / 1024)} KiB · 等比居中显示</p>}
    {targetError && <p role="status" className="wall-warning">{targetError}</p>}
    {pollError && <p role="alert" className="wall-error">{pollError}</p>}
    {error && <p role="alert" className="wall-error">{error}</p>}
    <div className="wall-toolbar">
      <button type="button" className="ghost-button" disabled={busy || !online} onClick={() => setRefreshKey((key) => key + 1)}>刷新终端</button>
      <button type="button" className="signal-button" disabled={busy || !online || !state || !file || !endpointId || Boolean(targetError || pollError)}
        onClick={() => void send()}>{busy ? "正在发送..." : "发送图片到所选终端"}</button>
    </div>
    {receipt && <p role="status">{state && state.revision >= receipt.revision
      ? deviceImageDelivery(state, receipt) : "已提交，正在读取接收状态"}</p>}
    <p className="wall-muted">此操作会在“屏幕墙”中创建独立单屏布局。停止显示可在屏幕墙中删除该布局；已有其他内容的墙面不会被覆盖。</p>
  </section>;
}
