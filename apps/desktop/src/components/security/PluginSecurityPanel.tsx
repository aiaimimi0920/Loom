// Owns plugin trust, credentials, and publisher identity controls.
import { Badge, Button, EmptyState, Field, IconButton, Input, Panel, Select } from "@neuro/ui";
import {
  deletePluginCredential,
  getPublisherIdentity,
  listPluginCredentials,
  listPluginTrust,
  type LoomCredentialDetails,
  type LoomCredentialSummary,
  type LoomPluginTrustPolicy,
  type LoomPluginTrustStore,
  type LoomPublisherIdentityState,
  revealPluginCredential,
  revealPublisherPrivateKey,
  rotatePublisherIdentity,
  savePluginCredential,
  setPluginTrustPolicy,
  trustPluginUser,
  untrustPluginUser,
} from "../../services/loomApi";
import { ArtIcon } from "../app/appShell";
import { CredentialFieldDialog } from "./CredentialFieldDialog";
import { CredentialFieldList } from "./CredentialFieldList";
import {
  CredentialFieldDraft,
  defaultPluginTrustStore,
  pushAppToast,
  requestAppConfirmation,
} from "../feedback/AppFeedback";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

export function PluginSecurityPanel({ baseUrl }: { baseUrl: string }) {
  return <PluginSecuritySession key={baseUrl} baseUrl={baseUrl} />;
}

// Drafts, secrets and busy state belong to one daemon; changing it starts a fresh UI session.
function PluginSecuritySession({ baseUrl }: { baseUrl: string }) {
  const [trust, setTrust] = useState<LoomPluginTrustStore>(defaultPluginTrustStore);
  const [credentials, setCredentials] = useState<LoomCredentialSummary[]>([]);
  const [identityState, setIdentityState] = useState<LoomPublisherIdentityState>({
    identity: null,
    hasPrivateKey: false,
  });
  const [privateKey, setPrivateKey] = useState("");
  const [showPrivateKey, setShowPrivateKey] = useState(false);
  const [trustedUserId, setTrustedUserId] = useState("");
  const [credentialDraft, setCredentialDraft] = useState<CredentialFieldDraft | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const loadVersion = useRef(0);
  const secretRevealVersion = useRef(0);
  const credentialCreateButton = useRef<HTMLButtonElement>(null);
  const credentialReturnFocus = useRef<HTMLButtonElement>(null);
  const trustedUserInput = useRef<HTMLInputElement>(null);
  const removedUserFocus = useRef<string | null>(null);

  useLayoutEffect(() => {
    if (busy !== null || !removedUserFocus.current) return;
    const removed = !trust.trustedPublishers.includes(removedUserFocus.current);
    removedUserFocus.current = null;
    if (removed && document.activeElement === document.body) trustedUserInput.current?.focus();
  }, [busy, trust.trustedPublishers]);

  const load = useCallback(async () => {
    const version = ++loadVersion.current;
    secretRevealVersion.current += 1;
    setPrivateKey("");
    setShowPrivateKey(false);
    try {
      const [nextTrust, summaries, nextIdentity] = await Promise.all([
        listPluginTrust(baseUrl),
        listPluginCredentials(baseUrl),
        getPublisherIdentity(baseUrl),
      ]);
      const visibleSummaries = summaries.filter(
        (credential) =>
          !credential.name.startsWith("loom-") && !credential.scope.frameworkId && !credential.scope.artId,
      );
      if (version !== loadVersion.current) return;
      setTrust(nextTrust);
      setCredentials(visibleSummaries);
      setIdentityState(nextIdentity);
    } catch (err) {
      if (version === loadVersion.current) {
        pushAppToast({ level: "error", text: err instanceof Error ? err.message : "无法读取密钥与安全设置。" });
      }
    }
  }, [baseUrl]);

  useEffect(() => {
    void load();
    return () => {
      loadVersion.current += 1;
      secretRevealVersion.current += 1;
    };
  }, [load]);

  const openNewCredential = () => {
    setCredentialDraft({
      name: "",
      value: "",
      valueType: "string",
    });
  };

  const openCredential = async (credential: LoomCredentialSummary) => {
    const revealVersion = ++secretRevealVersion.current;
    setBusy("credential-reveal");
    try {
      const details = await revealPluginCredential(baseUrl, credential.name, credential.scope);
      if (revealVersion !== secretRevealVersion.current) return;
      if (!details) throw new Error("凭据不存在或已被移除。");
      setCredentialDraft({
        name: details.name,
        value: details.value,
        valueType: details.valueType ?? "string",
        original: details,
      });
    } catch (err) {
      if (revealVersion === secretRevealVersion.current) {
        pushAppToast({ level: "error", text: err instanceof Error ? err.message : "无法读取凭据。" });
      }
    } finally {
      if (revealVersion === secretRevealVersion.current) setBusy(null);
    }
  };

  const togglePrivateKey = async () => {
    if (showPrivateKey) {
      setShowPrivateKey(false);
      setPrivateKey("");
      return;
    }
    if (!identityState.hasPrivateKey) return;
    const revealVersion = ++secretRevealVersion.current;
    setBusy("private-key");
    try {
      const revealed = await revealPublisherPrivateKey(baseUrl);
      if (revealVersion !== secretRevealVersion.current) return;
      setPrivateKey(revealed.privateKey);
      setShowPrivateKey(true);
    } catch (err) {
      if (revealVersion === secretRevealVersion.current) {
        pushAppToast({ level: "error", text: err instanceof Error ? err.message : "无法读取私钥。" });
      }
    } finally {
      if (revealVersion === secretRevealVersion.current) setBusy(null);
    }
  };

  const saveCredential = async () => {
    if (!credentialDraft) return;
    setBusy("credential");
    try {
      await savePluginCredential(baseUrl, {
        name: credentialDraft.name.trim(),
        value: credentialDraft.value,
        valueType: credentialDraft.valueType,
        scope: {},
      });
      const original = credentialDraft.original;
      if (original && original.name !== credentialDraft.name.trim()) {
        await deletePluginCredential(baseUrl, original.name, original.scope);
      }
      await load();
      setCredentialDraft(null);
      pushAppToast({ level: "info", text: "已保存" });
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "保存失败。" });
    } finally {
      setBusy(null);
    }
  };

  const removeCredential = async () => {
    const credential = credentialDraft?.original;
    if (!credential) return;
    const version = loadVersion.current;
    if (
      !(await requestAppConfirmation({
        title: "删除凭据",
        message: `删除 ${credential.name} 后，引用该凭据的 Art 可能无法运行。`,
        confirmLabel: "删除",
      }))
    )
      return;
    if (version !== loadVersion.current) return;
    setBusy("credential");
    try {
      await deletePluginCredential(baseUrl, credential.name, credential.scope);
      await load();
      setCredentialDraft(null);
      pushAppToast({ level: "info", text: "已删除" });
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "删除失败。" });
    } finally {
      setBusy(null);
    }
  };

  const updatePolicy = async (policy: LoomPluginTrustPolicy) => {
    setBusy("policy");
    try {
      setTrust(await setPluginTrustPolicy(baseUrl, policy));
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "策略保存失败。" });
    } finally {
      setBusy(null);
    }
  };

  const addTrustedUser = async () => {
    const userId = trustedUserId.trim();
    setBusy("trusted-user");
    try {
      setTrust(await trustPluginUser(baseUrl, userId));
      setTrustedUserId("");
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "添加可信用户失败。" });
    } finally {
      setBusy(null);
    }
  };

  const removeTrustedUser = async (userId: string) => {
    const version = loadVersion.current;
    if (
      !(await requestAppConfirmation({
        title: "移除可信用户",
        message: `从信任库移除 ${userId} 后，该用户后续发布的 Art 将不再被自动信任。`,
        confirmLabel: "移除",
      }))
    )
      return;
    if (version !== loadVersion.current) return;
    setBusy(userId);
    removedUserFocus.current = userId;
    try {
      setTrust(await untrustPluginUser(baseUrl, userId));
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "移除可信用户失败。" });
    } finally {
      setBusy(null);
    }
  };

  const resetIdentity = async () => {
    const version = loadVersion.current;
    if (
      !(await requestAppConfirmation({
        title: "重置密钥",
        message: "重置会生成新的私钥和公钥。平台仍需保留旧公钥，才能验证旧版本 Art。",
        confirmLabel: "重置",
      }))
    )
      return;
    if (version !== loadVersion.current) return;
    setBusy("identity");
    try {
      setIdentityState(await rotatePublisherIdentity(baseUrl));
      await load();
      pushAppToast({ level: "info", text: "密钥已重置" });
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "密钥重置失败。" });
    } finally {
      setBusy(null);
    }
  };

  const identity = identityState.identity;
  return (
    <div className="plugin-security">
      <section className="security-section security-section--credentials">
        <CredentialFieldList
          credentials={credentials}
          busy={busy !== null}
          createButton={credentialCreateButton}
          returnFocus={credentialReturnFocus}
          onCreate={openNewCredential}
          onEdit={(credential) => void openCredential(credential)}
        />
      </section>

      <section className="security-section security-section--policy">
        <div className="security-policy-row">
          <Field label="安装策略">
            <Select
              aria-label="Art 安装信任原则"
              value={trust.policy}
              disabled={busy !== null}
              onChange={(event) => void updatePolicy(event.target.value as LoomPluginTrustPolicy)}
            >
              <option value="require_signed">安装签名认证成功的</option>
              <option value="require_trusted">安装签名认证成功且在信任库中用户发布的 Art</option>
              <option value="allow_unsigned">可安装无签名 Art</option>
            </Select>
          </Field>
        </div>

        {trust.policy === "require_trusted" ? (
          <div className="trusted-user-library">
            <div className="trusted-user-library__add">
              <Input
                ref={trustedUserInput}
                aria-label="可信用户 ID"
                placeholder="L0000000000"
                value={trustedUserId}
                onChange={(event) => setTrustedUserId(event.target.value)}
              />
              <IconButton
                label="添加可信用户"
                title="添加"
                loading={busy === "trusted-user"}
                disabled={busy !== null || !/^(?:NU\d{11}|L\d{10})$/.test(trustedUserId.trim())}
                onClick={() => void addTrustedUser()}
              >
                <ArtIcon kind="plus" />
              </IconButton>
            </div>
            <div className="trusted-user-library__list">
              {trust.trustedPublishers.map((userId) => (
                <Panel className="trusted-user-library__row" key={userId}>
                  <code title={userId}>{userId}</code>
                  <Badge>
                    {trust.publishers.filter((key) => key.publisherId === userId && !key.revoked).length} keys
                  </Badge>
                  <IconButton
                    label={`移除 ${userId}`}
                    title="移除"
                    loading={busy === userId}
                    disabled={busy !== null}
                    onClick={() => void removeTrustedUser(userId)}
                  >
                    <ArtIcon kind="trash" />
                  </IconButton>
                </Panel>
              ))}
              {trust.trustedPublishers.length === 0 ? <EmptyState title="暂无可信用户" /> : null}
            </div>
          </div>
        ) : null}

        <div className="publisher-identity">
          <div className="publisher-identity__id-row">
            <div className="publisher-identity__id">
              <strong>用户 ID：</strong>
              <code title={identity?.userId ?? "L0000000000"}>{identity?.userId ?? "L0000000000"}</code>
            </div>
            <Button
              className="publisher-identity__action"
              loading={busy === "identity"}
              disabled={busy !== null}
              onClick={() => void resetIdentity()}
            >
              重置密钥
            </Button>
          </div>
          <div className="publisher-identity__keys">
            <div className="publisher-identity__secret">
              <Field label="私钥">
                <Input
                  className="mono-line"
                  readOnly
                  type={showPrivateKey ? "text" : "password"}
                  value={privateKey}
                  autoComplete="off"
                />
              </Field>
              <IconButton
                label={showPrivateKey ? "隐藏私钥" : "显示私钥"}
                aria-pressed={showPrivateKey}
                title={showPrivateKey ? "隐藏" : "显示"}
                loading={busy === "private-key"}
                disabled={busy !== null || !identityState.hasPrivateKey}
                onClick={() => void togglePrivateKey()}
              >
                <ArtIcon kind="eye" />
              </IconButton>
            </div>
            <Field label="公钥">
              <Input className="mono-line" readOnly value={identity?.publicKey ?? ""} />
            </Field>
          </div>
        </div>
      </section>

      <CredentialFieldDialog
        fallbackFocusRef={credentialCreateButton}
        returnFocusRef={credentialReturnFocus}
        draft={credentialDraft}
        busy={busy === "credential"}
        onChange={setCredentialDraft}
        onSave={() => void saveCredential()}
        onDelete={() => void removeCredential()}
        onClose={() => setCredentialDraft(null)}
      />
    </div>
  );
}
