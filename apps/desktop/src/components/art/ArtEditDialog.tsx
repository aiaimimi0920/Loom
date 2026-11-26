// Art-specific form composition; the standard dialog owns the modal lifecycle.
import { Badge, Button, Dialog, Disclosure, Input, Notice, Panel, Select, Switch, Textarea } from "@neuro/ui";
import { useRef } from "react";
import { useUiText } from "../../i18n/index.ts";
import { artDisplayIdentity } from "../../services/artHubUi";
import { ArtPublisherIcon } from "../app/appShell";
import { ArtEditParameter, ArtEditSecretParameter } from "./ArtEditParameter";
import type { ArtEditDialogProps } from "./artEditModel";
import { useArtEditDialogController } from "./useArtEditDialogController";

export function ArtEditDialog(props: ArtEditDialogProps) {
  const { tool, management, loading, busyAction, onClose } = props;
  const { t, tf, language } = useUiText();
  const controller = useArtEditDialogController(props);
  const {
    name,
    setName,
    description,
    setDescription,
    autoUpdate,
    setAutoUpdate,
    targetVersion,
    setTargetVersion,
    busy,
    visibleError,
    requiredParameters,
    optionalParameters,
    secretParameters,
    hasSplitWorkspace,
    submit,
  } = controller;
  const nameInputRef = useRef<HTMLInputElement | null>(null);
  if (!tool) return null;
  const displayIdentity = artDisplayIdentity(tool, management?.artId, language);
  return (
    <Dialog
      open
      title={t("art.action.edit")}
      closeLabel={t("art.edit.closeAria")}
      busy={busy}
      className="art-edit-dialog"
      initialFocusRef={nameInputRef}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <form
        className="art-edit-dialog__form"
        onSubmit={(event) => {
          event.preventDefault();
          void submit("save");
        }}
      >
        <div className="art-edit-dialog__scroll">
          {visibleError ? (
            <Notice tone="danger" role="alert">
              {visibleError}
            </Notice>
          ) : null}
          {loading || !management ? <Notice role="status">{t("art.common.loading")}</Notice> : null}
          {management ? (
            <>
              <div className="art-edit-dialog__overview">
                <Panel
                  className="art-edit-dialog__section art-edit-dialog__identity"
                  aria-label={t("art.edit.section.identity")}
                >
                  <div className="art-edit-dialog__identity-meta">
                    <span className="art-edit-dialog__publisher" title={displayIdentity.publisher.id}>
                      <ArtPublisherIcon publisher={displayIdentity.publisher} />
                      <strong>{displayIdentity.publisher.name}</strong>
                    </span>
                    <strong className="art-edit-dialog__english-name" title={displayIdentity.englishName}>
                      {displayIdentity.englishName}
                    </strong>
                    {displayIdentity.globalId ? (
                      <code
                        className="art-edit-dialog__id"
                        aria-label={tf("art.edit.globalIdAria", { id: displayIdentity.globalId })}
                        title={displayIdentity.globalId}
                      >
                        {displayIdentity.globalId}
                      </code>
                    ) : null}
                  </div>
                  <Input
                    className="art-edit-dialog__name"
                    ref={nameInputRef}
                    autoFocus={management.canEditIdentity && !busy}
                    aria-label={t("art.edit.field.name")}
                    placeholder={t("art.edit.placeholder.name")}
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                    disabled={busy || !management.canEditIdentity}
                    required
                  />
                  <Textarea
                    className="art-edit-dialog__description"
                    aria-label={t("art.edit.field.description")}
                    placeholder={t("art.edit.field.description")}
                    value={description}
                    onChange={(event) => setDescription(event.target.value)}
                    disabled={busy || !management.canEditIdentity}
                  />
                </Panel>
                <Panel
                  className="art-edit-dialog__section art-edit-dialog__version"
                  aria-label={t("art.edit.section.version")}
                >
                  <div className="art-edit-dialog__version-primary">
                    <div className="art-edit-dialog__version-state">
                      <code
                        className="art-edit-dialog__current-version"
                        aria-label={tf("art.edit.currentVersionAria", { version: management.currentVersion })}
                      >
                        {management.currentVersion}
                      </code>
                      {management.updateAvailable ? (
                        <Badge tone="info" aria-label={t("art.edit.updateAvailableAria")}>
                          {t("art.edit.updateAvailableBadge")}
                        </Badge>
                      ) : null}
                    </div>
                    <Switch
                      aria-label={t("art.edit.autoUpdate")}
                      label={t("art.edit.autoUpdate")}
                      checked={autoUpdate}
                      onChange={(event) => setAutoUpdate(event.target.checked)}
                      disabled={busy}
                    />
                  </div>
                  <div className="art-edit-dialog__version-action">
                    <Select
                      aria-label={t("art.edit.targetVersionAria")}
                      value={targetVersion}
                      onChange={(event) => setTargetVersion(event.target.value)}
                      disabled={busy || autoUpdate}
                    >
                      {management.availableVersions.map((version) => (
                        <option key={version} value={version}>
                          {version}
                        </option>
                      ))}
                    </Select>
                    <Button
                      variant="secondary"
                      type="button"
                      loading={busyAction === "update"}
                      disabled={busy || autoUpdate || !targetVersion || targetVersion === management.currentVersion}
                      onClick={() => void submit("update")}
                    >
                      {t("art.action.update")}
                    </Button>
                  </div>
                </Panel>
              </div>
              <div
                className={`art-edit-dialog__workspace${hasSplitWorkspace ? "" : " art-edit-dialog__workspace--single"}`}
              >
                {requiredParameters.length ? (
                  <Panel
                    className="art-edit-dialog__section art-edit-dialog__parameters"
                    aria-label={t("art.edit.section.parameters")}
                  >
                    <div className="art-edit-dialog__fields">
                      {requiredParameters.map((parameter) => (
                        <ArtEditParameter key={parameter.id} parameter={parameter} controller={controller} />
                      ))}
                    </div>
                  </Panel>
                ) : null}
                {secretParameters.length ? (
                  <section className="art-edit-dialog__secrets" aria-label={t("art.edit.section.secrets")}>
                    <div className="art-edit-dialog__secret-list">
                      {secretParameters.map((parameter) => (
                        <ArtEditSecretParameter key={parameter.id} parameter={parameter} controller={controller} />
                      ))}
                    </div>
                  </section>
                ) : null}
                {optionalParameters.length ? (
                  <Disclosure
                    className="art-edit-dialog__optional"
                    title={<span aria-label={t("art.edit.section.optionalAria")}>{t("art.edit.more")}</span>}
                  >
                    <div className="art-edit-dialog__fields art-edit-dialog__fields--optional">
                      {optionalParameters.map((parameter) => (
                        <ArtEditParameter key={parameter.id} parameter={parameter} controller={controller} />
                      ))}
                    </div>
                  </Disclosure>
                ) : null}
              </div>
            </>
          ) : null}
        </div>
        <div className="art-edit-dialog__actions">
          <Button variant="secondary" type="button" onClick={onClose} disabled={busy}>
            {t("art.action.cancel")}
          </Button>
          <Button
            variant="primary"
            type="submit"
            loading={busyAction === "save"}
            disabled={loading || !management || busy || (management.canEditIdentity && !name.trim())}
          >
            {t("art.action.save")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
