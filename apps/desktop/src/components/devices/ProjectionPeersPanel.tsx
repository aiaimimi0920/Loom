import { useEffect, useRef, useState } from "react";
import { Badge, Button, Disclosure, EmptyState, Field, Input, Notice, Panel, Switch } from "@neuro/ui";
import {
  listProjectionPeers,
  probeProjectionPeer,
  removeProjectionPeer,
  saveProjectionPeer,
  type ProjectionPeer,
  type ProjectionPeerState,
} from "../../services/loomApi/projectionPeers.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";
import { requestAppConfirmation } from "../feedback/AppFeedback";
import "./ProjectionPeersPanel.css";
import "./DeviceConnectionLayout.css";

const emptyPeer = (): ProjectionPeer => ({ peerId: "", publicKey: "", name: "", origin: "", enabled: true });
const trustWarning = "修改任一对等连接会终止此 Loom 现有的跨 Loom 投射关联，需要重新投送。请先核对对方公开身份。";

export function ProjectionPeersPanel({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const [state, setState] = useState<ProjectionPeerState | null>(null);
  const [draft, setDraft] = useState<ProjectionPeer>(emptyPeer);
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const active = useRef(0);
  const inFlight = useRef(false);
  useEffect(() => {
    const generation = ++active.current;
    setState(null);
    setError("");
    setNotice("");
    setDraft(emptyPeer());
    setEditing(false);
    inFlight.current = online;
    setBusy(online);
    if (online)
      void listProjectionPeers(baseUrl)
        .then((next) => {
          if (active.current === generation) setState(next);
        })
        .catch((reason: unknown) => {
          if (active.current === generation) setError(errorMessage(reason));
        })
        .finally(() => {
          if (active.current === generation) {
            inFlight.current = false;
            setBusy(false);
          }
        });
    return () => {
      active.current++;
    };
  }, [baseUrl, online]);

  async function run(action: () => Promise<void>) {
    if (!online || inFlight.current) return;
    const generation = active.current;
    inFlight.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await action();
    } catch (reason) {
      if (active.current === generation) setError(errorMessage(reason) + "。请刷新配置后重试；不会自动覆盖并发修改。");
    } finally {
      if (active.current === generation) {
        inFlight.current = false;
        setBusy(false);
      }
    }
  }
  function validGeneration(generation: number) {
    return active.current === generation;
  }
  async function refresh() {
    const generation = active.current;
    const next = await listProjectionPeers(baseUrl);
    if (validGeneration(generation)) setState(next);
  }
  async function mutate(remove: boolean) {
    if (!state) return;
    const generation = active.current;
    const accepted = await requestAppConfirmation({
      title: remove ? "移除对等 Loom" : "保存对等 Loom",
      message: trustWarning,
      confirmLabel: remove ? "移除" : "保存",
    });
    if (!accepted || !validGeneration(generation)) return;
    const next = remove
      ? await removeProjectionPeer(baseUrl, state.revision, draft.peerId)
      : await saveProjectionPeer(baseUrl, state.revision, {
          ...draft,
          name: draft.name.trim(),
          origin: draft.origin.trim(),
          peerId: draft.peerId.trim(),
          publicKey: draft.publicKey.trim(),
          certificate: draft.certificate?.trim() || undefined,
        });
    if (!validGeneration(generation)) return;
    setState(next);
    setDraft(emptyPeer());
    setEditing(false);
    setNotice(remove ? "对等连接已移除。" : "连接配置已保存。请在对方 Loom 配置本机公开身份，再验证连接。");
  }
  async function probe(peer: ProjectionPeer) {
    if (!state) return;
    const generation = active.current;
    await probeProjectionPeer(baseUrl, state.revision, peer.peerId);
    if (validGeneration(generation)) setNotice(peer.name + "：已验证双方信任及签名连接。本次检查不表示贴图已经送达。");
  }
  const disabled = busy || !online;
  return (
    <section className="loom-device-settings projection-peers" aria-label="投射连接管理" aria-busy={busy || undefined}>
      <header>
        <h2>跨 Loom 投射连接</h2>
        <Button variant="quiet" disabled={disabled} onClick={() => void run(refresh)}>
          刷新配置
        </Button>
      </header>
      {!online && <Notice tone="warning">本机 Loom 离线。</Notice>}
      {error && <Notice tone="danger">{error}</Notice>}
      {notice && <Notice tone="success">{notice}</Notice>}
      {state && (
        <>
          <Disclosure title="本机公开身份">
            <div className="loom-device-settings__section">
              <Field label="身份指纹">
                <Input readOnly value={state.identity.peerId} />
              </Field>
              <Field label="公钥">
                <Input readOnly value={state.identity.publicKey} />
              </Field>
              {state.localConnection && <>
                <Field label="本机 Loom 地址"><Input readOnly value={state.localConnection.origin} /></Field>
                <Field label="本机 TLS 证书"><Input readOnly value={state.localConnection.certificate} /></Field>
              </>}
              <Notice tone="warning">仅交换公开身份和公钥，不交换管理员令牌或私钥文件。</Notice>
            </div>
          </Disclosure>
          <div className="loom-device-settings__metadata">
            <Badge>{state.peers.length} / 16 个连接</Badge>
            <code>版本 {state.revision}</code>
          </div>
          {state.peers.length === 0 && <EmptyState title="尚未添加其他 Loom" />}
          {state.peers.map((peer) => (
            <Panel className="projection-peer" key={peer.peerId}>
              <div className="projection-peer__identity">
                <strong>{peer.name}</strong>
                <Badge tone={peer.enabled ? "success" : "neutral"}>{peer.enabled ? "已启用" : "已停用"}</Badge>
                <code>{peer.origin}</code>
              </div>
              <div className="loom-device-settings__actions">
                <Button
                  variant="quiet"
                  aria-label={`编辑 ${peer.name}`}
                  disabled={disabled}
                  onClick={() => {
                    setDraft({ ...peer });
                    setEditing(true);
                    setNotice("");
                  }}
                >
                  编辑
                </Button>
                <Button
                  aria-label={`验证 ${peer.name}`}
                  disabled={disabled || !peer.enabled}
                  onClick={() => void run(() => probe(peer))}
                >
                  验证
                </Button>
              </div>
            </Panel>
          ))}
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void run(() => mutate(false));
            }}
          >
            <h3>{editing ? "编辑连接" : "添加连接"}</h3>
            <fieldset className="loom-device-settings__fields" disabled={disabled}>
              <Field label="名称">
                <Input
                  required
                  maxLength={128}
                  value={draft.name}
                  onChange={(event) => setDraft({ ...draft, name: event.target.value })}
                />
              </Field>
              <Field label="Loom 地址">
                <Input
                  required
                  maxLength={256}
                  placeholder="https://loom.example.test"
                  value={draft.origin}
                  onChange={(event) => setDraft({ ...draft, origin: event.target.value })}
                />
              </Field>
              <Field label="对方身份指纹">
                <Input
                  required
                  readOnly={editing}
                  maxLength={69}
                  value={draft.peerId}
                  onChange={(event) => setDraft({ ...draft, peerId: event.target.value })}
                />
              </Field>
              <Field label="对方公钥">
                <Input
                  required
                  readOnly={editing}
                  maxLength={44}
                  value={draft.publicKey}
                  onChange={(event) => setDraft({ ...draft, publicKey: event.target.value })}
                />
              </Field>
              <Field label="对方 TLS 证书（自签证书必填）">
                <Input maxLength={8192} value={draft.certificate ?? ""}
                  onChange={(event) => setDraft({ ...draft, certificate: event.target.value })} />
              </Field>
              <Switch
                className="loom-device-settings__wide"
                label="启用连接"
                checked={draft.enabled}
                onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })}
              />
            </fieldset>
            <div className="loom-device-settings__actions">
              <Button type="submit" variant="primary" disabled={disabled || (!editing && state.peers.length >= 16)}>
                保存连接
              </Button>
              {editing && (
                <Button variant="danger" disabled={disabled} onClick={() => void run(() => mutate(true))}>
                  移除连接
                </Button>
              )}
              <Button
                variant="quiet"
                disabled={disabled}
                onClick={() => {
                  setDraft(emptyPeer());
                  setEditing(false);
                }}
              >
                清空编辑
              </Button>
            </div>
          </form>
        </>
      )}
    </section>
  );
}
