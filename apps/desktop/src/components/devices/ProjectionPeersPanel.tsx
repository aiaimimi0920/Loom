import { useEffect, useRef, useState } from "react";
import { Badge, Button, Disclosure, EmptyState, Field, Input, Notice, Panel, Switch } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
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

export function ProjectionPeersPanel({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const { t, tf } = useUiText();
  const trustWarning = t("devices.projectionPeers.trustWarning");
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
      if (active.current === generation) {
        setError(tf("devices.projectionPeers.error.withRetryHint", { reason: errorMessage(reason) }));
      }
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
      title: remove ? t("devices.projectionPeers.confirmRemove.title") : t("devices.projectionPeers.confirmSave.title"),
      message: trustWarning,
      confirmLabel: remove ? t("devices.projectionPeers.confirmRemove.confirm") : t("devices.common.save"),
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
    setNotice(remove ? t("devices.projectionPeers.notice.removed") : t("devices.projectionPeers.notice.saved"));
  }
  async function probe(peer: ProjectionPeer) {
    if (!state) return;
    const generation = active.current;
    await probeProjectionPeer(baseUrl, state.revision, peer.peerId);
    if (validGeneration(generation)) setNotice(tf("devices.projectionPeers.notice.verified", { name: peer.name }));
  }
  const disabled = busy || !online;
  return (
    <section
      className="loom-device-settings projection-peers"
      aria-label={t("devices.projectionPeers.ariaLabel")}
      aria-busy={busy || undefined}
    >
      <header>
        <h2>{t("devices.projectionPeers.heading")}</h2>
        <Button variant="quiet" disabled={disabled} onClick={() => void run(refresh)}>
          {t("devices.projectionPeers.refreshAction")}
        </Button>
      </header>
      {!online && <Notice tone="warning">{t("devices.projection.localOfflineNotice")}</Notice>}
      {error && <Notice tone="danger">{error}</Notice>}
      {notice && <Notice tone="success">{notice}</Notice>}
      {state && (
        <>
          <Disclosure title={t("devices.projectionPeers.localIdentityTitle")}>
            <div className="loom-device-settings__section">
              <Field label={t("devices.projectionPeers.fingerprintLabel")}>
                <Input readOnly value={state.identity.peerId} />
              </Field>
              <Field label={t("devices.projectionPeers.publicKeyLabel")}>
                <Input readOnly value={state.identity.publicKey} />
              </Field>
              {state.localConnection && <>
                <Field label={t("devices.projectionPeers.localOriginLabel")}>
                  <Input readOnly value={state.localConnection.origin} />
                </Field>
                <Field label={t("devices.projectionPeers.localCertLabel")}>
                  <Input readOnly value={state.localConnection.certificate} />
                </Field>
              </>}
              <Notice tone="warning">{t("devices.projectionPeers.publicOnlyWarning")}</Notice>
            </div>
          </Disclosure>
          <div className="loom-device-settings__metadata">
            <Badge>{tf("devices.projectionPeers.countBadge", { count: state.peers.length, max: 16 })}</Badge>
            <code>{tf("devices.common.revisionBadge", { revision: state.revision })}</code>
          </div>
          {state.peers.length === 0 && <EmptyState title={t("devices.projectionPeers.empty")} />}
          {state.peers.map((peer) => (
            <Panel className="projection-peer" key={peer.peerId}>
              <div className="projection-peer__identity">
                <strong>{peer.name}</strong>
                <Badge tone={peer.enabled ? "success" : "neutral"}>
                  {peer.enabled ? t("devices.status.enabled") : t("devices.status.deactivated")}
                </Badge>
                <code>{peer.origin}</code>
              </div>
              <div className="loom-device-settings__actions">
                <Button
                  variant="quiet"
                  aria-label={tf("devices.projectionPeers.editPeerAriaLabel", { name: peer.name })}
                  disabled={disabled}
                  onClick={() => {
                    setDraft({ ...peer });
                    setEditing(true);
                    setNotice("");
                  }}
                >
                  {t("devices.common.edit")}
                </Button>
                <Button
                  aria-label={tf("devices.projectionPeers.verifyPeerAriaLabel", { name: peer.name })}
                  disabled={disabled || !peer.enabled}
                  onClick={() => void run(() => probe(peer))}
                >
                  {t("devices.projectionPeers.verifyAction")}
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
            <h3>{editing ? t("devices.projectionPeers.editHeading") : t("devices.projectionPeers.addHeading")}</h3>
            <fieldset className="loom-device-settings__fields" disabled={disabled}>
              <Field label={t("devices.common.name")}>
                <Input
                  required
                  maxLength={128}
                  value={draft.name}
                  onChange={(event) => setDraft({ ...draft, name: event.target.value })}
                />
              </Field>
              <Field label={t("devices.projectionPeers.peerOriginLabel")}>
                <Input
                  required
                  maxLength={256}
                  placeholder="https://loom.example.test"
                  value={draft.origin}
                  onChange={(event) => setDraft({ ...draft, origin: event.target.value })}
                />
              </Field>
              <Field label={t("devices.projectionPeers.peerFingerprintLabel")}>
                <Input
                  required
                  readOnly={editing}
                  maxLength={69}
                  value={draft.peerId}
                  onChange={(event) => setDraft({ ...draft, peerId: event.target.value })}
                />
              </Field>
              <Field label={t("devices.projectionPeers.peerPublicKeyLabel")}>
                <Input
                  required
                  readOnly={editing}
                  maxLength={44}
                  value={draft.publicKey}
                  onChange={(event) => setDraft({ ...draft, publicKey: event.target.value })}
                />
              </Field>
              <Field label={t("devices.projectionPeers.peerCertLabel")}>
                <Input maxLength={8192} value={draft.certificate ?? ""}
                  onChange={(event) => setDraft({ ...draft, certificate: event.target.value })} />
              </Field>
              <Switch
                className="loom-device-settings__wide"
                label={t("devices.projectionPeers.enablePeerSwitch")}
                checked={draft.enabled}
                onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })}
              />
            </fieldset>
            <div className="loom-device-settings__actions">
              <Button type="submit" variant="primary" disabled={disabled || (!editing && state.peers.length >= 16)}>
                {t("devices.projectionPeers.saveAction")}
              </Button>
              {editing && (
                <Button variant="danger" disabled={disabled} onClick={() => void run(() => mutate(true))}>
                  {t("devices.projectionPeers.removeAction")}
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
                {t("devices.projectionPeers.clearAction")}
              </Button>
            </div>
          </form>
        </>
      )}
    </section>
  );
}
