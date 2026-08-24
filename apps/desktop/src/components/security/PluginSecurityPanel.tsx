// Owns plugin trust, credentials, and publisher identity controls.
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
import { CredentialFieldDialog } from "../devices/DeviceManagementPanel";
import {
  CredentialFieldDraft,
  credentialFieldId,
  credentialValueTypeLabels,
  defaultPluginTrustStore,
  pushAppToast,
  requestAppConfirmation,
} from "../feedback/AppFeedback";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";

export function PluginSecurityPanel({ baseUrl }: { baseUrl: string }) {
  const [trust, setTrust] = useState<LoomPluginTrustStore>(defaultPluginTrustStore);
  const [credentials, setCredentials] = useState<LoomCredentialSummary[]>([]);
  const [identityState, setIdentityState] = useState<LoomPublisherIdentityState>({ identity: null, hasPrivateKey: false });
  const [privateKey, setPrivateKey] = useState("");
  const [showPrivateKey, setShowPrivateKey] = useState(false);
  const [trustedUserId, setTrustedUserId] = useState("");
  const [credentialDraft, setCredentialDraft] = useState<CredentialFieldDraft | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const loadVersion = useRef(0);
  const secretRevealVersion = useRef(0);

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
      const visibleSummaries = summaries.filter((credential) => (
        !credential.name.startsWith("loom-")
        && !credential.scope.frameworkId
        && !credential.scope.artId
      ));
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
      setCredentialDraft(null);
      await load();
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
    if (!await requestAppConfirmation({
      title: "删除凭据",
      message: `删除 ${credential.name} 后，引用该凭据的 Art 可能无法运行。`,
      confirmLabel: "删除",
    })) return;
    setBusy("credential");
    try {
      await deletePluginCredential(baseUrl, credential.name, credential.scope);
      setCredentialDraft(null);
      await load();
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
    if (!await requestAppConfirmation({
      title: "移除可信用户",
      message: `从信任库移除 ${userId} 后，该用户后续发布的 Art 将不再被自动信任。`,
      confirmLabel: "移除",
    })) return;
    setBusy(userId);
    try {
      setTrust(await untrustPluginUser(baseUrl, userId));
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : "移除可信用户失败。" });
    } finally {
      setBusy(null);
    }
  };

  const resetIdentity = async () => {
    if (!await requestAppConfirmation({
      title: "重置密钥",
      message: "重置会生成新的私钥和公钥。平台仍需保留旧公钥，才能验证旧版本 Art。",
      confirmLabel: "重置",
    })) return;
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
        <div className="credential-field-list" role="list">
          <div className="credential-field-row credential-field-row--head">
            <span>名字</span><span>格式</span><span>值</span>
            <button className="icon-button" type="button" aria-label="添加字段" title="添加字段" disabled={busy !== null} onClick={openNewCredential}>
              <ArtIcon kind="plus" />
            </button>
          </div>
          {credentials.map((credential) => (
            <div className="credential-field-row" role="listitem" key={credentialFieldId(credential)}>
              <strong title={credential.name}>{credential.name}</strong>
              <span>{credentialValueTypeLabels[credential.valueType ?? "string"]}</span>
              <code aria-label={`${credential.name} 已安全保存`}>••••••••</code>
              <button className="icon-button" type="button" aria-label={`编辑 ${credential.name}`} title="编辑" disabled={busy !== null} onClick={() => void openCredential(credential)}>
                <ArtIcon kind="edit" />
              </button>
            </div>
          ))}
          {credentials.length === 0 ? <p className="muted-line">暂无字段</p> : null}
        </div>
      </section>

      <section className="security-section security-section--policy">
        <div className="security-policy-row">
          <strong>安装策略</strong>
          <select className="studio-input security-policy-select" aria-label="Art 安装信任原则" value={trust.policy} disabled={busy !== null} onChange={(event) => void updatePolicy(event.target.value as LoomPluginTrustPolicy)}>
            <option value="require_signed">安装签名认证成功的</option>
            <option value="require_trusted">安装签名认证成功且在信任库中用户发布的 Art</option>
            <option value="allow_unsigned">可安装无签名 Art</option>
          </select>
        </div>

        {trust.policy === "require_trusted" ? (
          <div className="trusted-user-library">
            <div className="trusted-user-library__add">
              <input className="studio-input" aria-label="可信用户 ID" placeholder="L0000000000" value={trustedUserId} onChange={(event) => setTrustedUserId(event.target.value)} />
              <button className="icon-button" type="button" aria-label="添加可信用户" title="添加" disabled={busy !== null || !/^(?:NU\d{11}|L\d{10})$/.test(trustedUserId.trim())} onClick={() => void addTrustedUser()}>
                <ArtIcon kind="plus" />
              </button>
            </div>
            <div className="trusted-user-library__list">
              {trust.trustedPublishers.map((userId) => (
                <div className="trusted-user-library__row" key={userId}>
                  <code>{userId}</code>
                  <span>{trust.publishers.filter((key) => key.publisherId === userId && !key.revoked).length} keys</span>
                  <button className="icon-button" type="button" aria-label={`移除 ${userId}`} title="移除" disabled={busy !== null} onClick={() => void removeTrustedUser(userId)}>
                    <ArtIcon kind="trash" />
                  </button>
                </div>
              ))}
              {trust.trustedPublishers.length === 0 ? <p className="muted-line">暂无可信用户</p> : null}
            </div>
          </div>
        ) : null}

        <div className="publisher-identity">
          <div className="publisher-identity__id-row">
            <div className="publisher-identity__id">
              <strong>用户 ID：</strong>
              <code>{identity?.userId ?? "L0000000000"}</code>
            </div>
            <button className="ghost-button publisher-identity__action" type="button" disabled={busy !== null} onClick={() => void resetIdentity()}>
              {busy === "identity" ? "重置中" : "重置密钥"}
            </button>
          </div>
          <div className="publisher-identity__keys">
            <label>
              <span>私钥</span>
              <div className="publisher-identity__secret">
                <input className="studio-input mono-line" readOnly type={showPrivateKey ? "text" : "password"} value={privateKey} />
                <button className="icon-button" type="button" aria-label={showPrivateKey ? "隐藏私钥" : "显示私钥"} title={showPrivateKey ? "隐藏" : "显示"} disabled={busy !== null || !identityState.hasPrivateKey} onClick={() => void togglePrivateKey()}>
                  <ArtIcon kind="eye" />
                </button>
              </div>
            </label>
            <label>
              <span>公钥</span>
              <input className="studio-input mono-line" readOnly value={identity?.publicKey ?? ""} />
            </label>
          </div>
        </div>
      </section>

      <CredentialFieldDialog
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
