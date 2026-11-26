// Composes the global credential summary table; revealing and mutations belong to the security owner.
import { DataTable, EmptyState, IconButton } from "@neuro/ui";
import type { RefObject } from "react";
import { useUiText } from "../../i18n/index.ts";
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
  const { t, tf } = useUiText();
  return (
    <div className="credential-field-list">
      <DataTable
        label={t("security.credential.tableLabel")}
        rows={credentials}
        getRowKey={credentialFieldId}
        empty={<EmptyState title={t("security.credential.empty")} />}
        columns={[
          {
            key: "name",
            header: t("security.credential.columnName"),
            rowHeader: true,
            width: "35%",
            cell: (credential) => <strong title={credential.name}>{credential.name}</strong>,
          },
          {
            key: "format",
            header: t("security.credential.columnFormat"),
            width: "20%",
            cell: (credential) => credentialValueTypeLabels[credential.valueType ?? "string"],
          },
          {
            key: "value",
            header: t("security.credential.columnValue"),
            cell: (credential) => (
              <code aria-label={tf("security.credential.savedAria", { name: credential.name })}>••••••••</code>
            ),
          },
          {
            key: "action",
            width: "56px",
            header: (
              <IconButton
                ref={createButton}
                label={t("security.credential.addTitle")}
                title={t("security.credential.addTitle")}
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
                label={tf("security.credential.editAria", { name: credential.name })}
                title={t("common.edit")}
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
