// Art-specific form composition; the standard dialog owns the modal lifecycle.
import { Badge, Button, Dialog, Disclosure, Input, Notice, Panel, Select, Switch, Textarea } from "@neuro/ui";
import { useRef } from "react";
import { artDisplayIdentity } from "../../services/artHubUi";
import { ArtPublisherIcon } from "../app/appShell";
import { ArtEditParameter, ArtEditSecretParameter } from "./ArtEditParameter";
import type { ArtEditDialogProps } from "./artEditModel";
import { useArtEditDialogController } from "./useArtEditDialogController";

export function ArtEditDialog(props: ArtEditDialogProps) {
  const { tool, management, loading, busyAction, onClose } = props;
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
  const displayIdentity = artDisplayIdentity(
    tool,
    management?.artId,
    typeof document === "undefined" ? "zh-CN" : document.documentElement.lang || window.navigator.language,
  );
  return (
    <Dialog
      open
      title="编辑"
      closeLabel="关闭编辑"
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
          {loading || !management ? <Notice role="status">加载中</Notice> : null}
          {management ? (
            <>
              <div className="art-edit-dialog__overview">
                <Panel className="art-edit-dialog__section art-edit-dialog__identity" aria-label="Art 信息">
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
                        aria-label={`Art 编号 ${displayIdentity.globalId}`}
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
                    aria-label="名称"
                    placeholder="Art 名称"
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                    disabled={busy || !management.canEditIdentity}
                    required
                  />
                  <Textarea
                    className="art-edit-dialog__description"
                    aria-label="描述"
                    placeholder="描述"
                    value={description}
                    onChange={(event) => setDescription(event.target.value)}
                    disabled={busy || !management.canEditIdentity}
                  />
                </Panel>
                <Panel className="art-edit-dialog__section art-edit-dialog__version" aria-label="版本">
                  <div className="art-edit-dialog__version-primary">
                    <div className="art-edit-dialog__version-state">
                      <code
                        className="art-edit-dialog__current-version"
                        aria-label={`当前版本 ${management.currentVersion}`}
                      >
                        {management.currentVersion}
                      </code>
                      {management.updateAvailable ? (
                        <Badge tone="info" aria-label="有新版本">
                          有更新
                        </Badge>
                      ) : null}
                    </div>
                    <Switch
                      aria-label="自动更新"
                      label="自动更新"
                      checked={autoUpdate}
                      onChange={(event) => setAutoUpdate(event.target.checked)}
                      disabled={busy}
                    />
                  </div>
                  <div className="art-edit-dialog__version-action">
                    <Select
                      aria-label="目标版本"
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
                      更新
                    </Button>
                  </div>
                </Panel>
              </div>
              <div
                className={`art-edit-dialog__workspace${hasSplitWorkspace ? "" : " art-edit-dialog__workspace--single"}`}
              >
                {requiredParameters.length ? (
                  <Panel className="art-edit-dialog__section art-edit-dialog__parameters" aria-label="参数">
                    <div className="art-edit-dialog__fields">
                      {requiredParameters.map((parameter) => (
                        <ArtEditParameter key={parameter.id} parameter={parameter} controller={controller} />
                      ))}
                    </div>
                  </Panel>
                ) : null}
                {secretParameters.length ? (
                  <section className="art-edit-dialog__secrets" aria-label="机密参数">
                    <div className="art-edit-dialog__secret-list">
                      {secretParameters.map((parameter) => (
                        <ArtEditSecretParameter key={parameter.id} parameter={parameter} controller={controller} />
                      ))}
                    </div>
                  </section>
                ) : null}
                {optionalParameters.length ? (
                  <Disclosure className="art-edit-dialog__optional" title={<span aria-label="可选参数">更多</span>}>
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
            取消
          </Button>
          <Button
            variant="primary"
            type="submit"
            loading={busyAction === "save"}
            disabled={loading || !management || busy || (management.canEditIdentity && !name.trim())}
          >
            保存
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
