// Owns plugin trust, credentials, and publisher identity controls.
import { Badge, Button, EmptyState, Field, IconButton, Input, Panel, Select } from "@neuro/ui";
import { uiText, useUiText } from "../../i18n/index.ts";
import {
  deletePluginCredential,
  getPublisherIdentity,
  listPluginCredentials,
  listPluginTrust,
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
  const { t, tf } = useUiText();
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
        pushAppToast({ level: "error", text: err instanceof Error ? err.message : uiText("security.error.loadFailed") });
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
      if (!details) throw new Error(t("security.error.credentialNotFound"));
      setCredentialDraft({
        name: details.name,
        value: details.value,
        valueType: details.valueType ?? "string",
        original: details,
      });
    } catch (err) {
      if (revealVersion === secretRevealVersion.current) {
        pushAppToast({ level: "error", text: err instanceof Error ? err.message : t("security.error.revealCredentialFailed") });
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
        pushAppToast({ level: "error", text: err instanceof Error ? err.message : t("security.error.revealPrivateKeyFailed") });
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
      pushAppToast({ level: "info", text: t("common.saved") });
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : t("security.error.saveFailed") });
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
        title: t("security.credential.deleteConfirmTitle"),
        message: tf("security.credential.deleteConfirmMessage", { name: credential.name }),
        confirmLabel: t("common.delete"),
      }))
    )
      return;
    if (version !== loadVersion.current) return;
    setBusy("credential");
    try {
      await deletePluginCredential(baseUrl, credential.name, credential.scope);
      await load();
      setCredentialDraft(null);
      pushAppToast({ level: "info", text: t("common.deleted") });
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : t("security.error.deleteFailed") });
    } finally {
      setBusy(null);
    }
  };

  const updatePolicy = async (policy: LoomPluginTrustPolicy) => {
    setBusy("policy");
    try {
      setTrust(await setPluginTrustPolicy(baseUrl, policy));
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : t("security.error.savePolicyFailed") });
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
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : t("security.error.addTrustedUserFailed") });
    } finally {
      setBusy(null);
    }
  };

  const removeTrustedUser = async (userId: string) => {
    const version = loadVersion.current;
    if (
      !(await requestAppConfirmation({
        title: t("security.trustedUser.removeConfirmTitle"),
        message: tf("security.trustedUser.removeConfirmMessage", { userId }),
        confirmLabel: t("common.remove"),
      }))
    )
      return;
    if (version !== loadVersion.current) return;
    setBusy(userId);
    removedUserFocus.current = userId;
    try {
      setTrust(await untrustPluginUser(baseUrl, userId));
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : t("security.error.removeTrustedUserFailed") });
    } finally {
      setBusy(null);
    }
  };

  const resetIdentity = async () => {
    const version = loadVersion.current;
    if (
      !(await requestAppConfirmation({
        title: t("security.identity.resetConfirmTitle"),
        message: t("security.identity.resetConfirmMessage"),
        confirmLabel: t("common.reset"),
      }))
    )
      return;
    if (version !== loadVersion.current) return;
    setBusy("identity");
    try {
      setIdentityState(await rotatePublisherIdentity(baseUrl));
      await load();
      pushAppToast({ level: "info", text: t("security.identity.resetSuccess") });
    } catch (err) {
      pushAppToast({ level: "error", text: err instanceof Error ? err.message : t("security.error.resetKeysFailed") });
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
          <Field label={t("security.policy.label")}>
            <Select
              aria-label={t("security.policy.ariaLabel")}
              value={trust.policy}
              disabled={busy !== null}
              onChange={(event) => void updatePolicy(event.target.value as LoomPluginTrustPolicy)}
            >
              <option value="require_signed">{t("security.policy.requireSigned")}</option>
              <option value="require_trusted">{t("security.policy.requireTrusted")}</option>
              <option value="allow_unsigned">{t("security.policy.allowUnsigned")}</option>
            </Select>
          </Field>
        </div>

        {trust.policy === "require_trusted" ? (
          <div className="trusted-user-library">
            <div className="trusted-user-library__add">
              <Input
                ref={trustedUserInput}
                aria-label={t("security.trustedUser.idAria")}
                placeholder="L0000000000"
                value={trustedUserId}
                onChange={(event) => setTrustedUserId(event.target.value)}
              />
              <IconButton
                label={t("security.trustedUser.addAria")}
                title={t("common.add")}
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
                    {tf("security.trustedUser.keyCount", {
                      count: trust.publishers.filter((key) => key.publisherId === userId && !key.revoked).length,
                    })}
                  </Badge>
                  <IconButton
                    label={tf("security.trustedUser.removeAria", { userId })}
                    title={t("common.remove")}
                    loading={busy === userId}
                    disabled={busy !== null}
                    onClick={() => void removeTrustedUser(userId)}
                  >
                    <ArtIcon kind="trash" />
                  </IconButton>
                </Panel>
              ))}
              {trust.trustedPublishers.length === 0 ? <EmptyState title={t("security.trustedUser.empty")} /> : null}
            </div>
          </div>
        ) : null}

        <div className="publisher-identity">
          <div className="publisher-identity__id-row">
            <div className="publisher-identity__id">
              <strong>{t("security.identity.userIdLabel")}</strong>
              <code title={identity?.userId ?? "L0000000000"}>{identity?.userId ?? "L0000000000"}</code>
            </div>
            <Button
              className="publisher-identity__action"
              loading={busy === "identity"}
              disabled={busy !== null}
              onClick={() => void resetIdentity()}
            >
              {t("security.identity.resetAction")}
            </Button>
          </div>
          <div className="publisher-identity__keys">
            <div className="publisher-identity__secret">
              <Field label={t("security.identity.privateKey")}>
                <Input
                  className="mono-line"
                  readOnly
                  type={showPrivateKey ? "text" : "password"}
                  value={privateKey}
                  autoComplete="off"
                />
              </Field>
              <IconButton
                label={showPrivateKey ? t("security.identity.hidePrivateKey") : t("security.identity.showPrivateKey")}
                aria-pressed={showPrivateKey}
                title={showPrivateKey ? t("common.hide") : t("common.show")}
                loading={busy === "private-key"}
                disabled={busy !== null || !identityState.hasPrivateKey}
                onClick={() => void togglePrivateKey()}
              >
                <ArtIcon kind="eye" />
              </IconButton>
            </div>
            <Field label={t("security.identity.publicKey")}>
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
