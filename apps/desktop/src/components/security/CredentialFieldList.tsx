// Composes the global credential summary table; revealing and mutations belong to the security owner.
import { DataTable, EmptyState, IconButton } from "@neuro/ui";
import type { RefObject } from "react";
import type { LoomCredentialSummary } from "../../services/loomApi";
import { ArtIcon } from "../app/appShell";
import { credentialFieldId, credentialValueTypeLabels } from "../feedback/AppFeedback";

export function CredentialFieldList({
  credentials,
  busy,
  createButton,
  returnFocus,
  onCreate,
  onEdit,
}: {
  credentials: LoomCredentialSummary[];
  busy: boolean;
  createButton: RefObject<HTMLButtonElement | null>;
  returnFocus: RefObject<HTMLButtonElement | null>;
  onCreate: () => void;
  onEdit: (credential: LoomCredentialSummary) => void;
}) {
  return (
    <div className="credential-field-list">
      <DataTable
        label="全局凭据字段"
        rows={credentials}
        getRowKey={credentialFieldId}
        empty={<EmptyState title="暂无字段" />}
        columns={[
          {
            key: "name",
            header: "名字",
            rowHeader: true,
            width: "35%",
            cell: (credential) => <strong title={credential.name}>{credential.name}</strong>,
          },
          {
            key: "format",
            header: "格式",
            width: "20%",
            cell: (credential) => credentialValueTypeLabels[credential.valueType ?? "string"],
          },
          {
            key: "value",
            header: "值",
            cell: (credential) => <code aria-label={`${credential.name} 已安全保存`}>••••••••</code>,
          },
          {
            key: "action",
            width: "56px",
            header: (
              <IconButton
                ref={createButton}
                label="添加字段"
                title="添加字段"
                disabled={busy}
                onClick={(event) => {
                  returnFocus.current = event.currentTarget;
                  onCreate();
                }}
              >
                <ArtIcon kind="plus" />
              </IconButton>
            ),
            cell: (credential) => (
              <IconButton
                label={`编辑 ${credential.name}`}
                title="编辑"
                disabled={busy}
                onClick={(event) => {
                  returnFocus.current = event.currentTarget;
                  onEdit(credential);
                }}
              >
                <ArtIcon kind="edit" />
              </IconButton>
            ),
          },
        ]}
      />
    </div>
  );
}
