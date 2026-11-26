// Framework identity and filter selection remain application-owned.
import { Checkbox, MultiSelect, Toolbar } from "@neuro/ui";
import type { ReactNode } from "react";
import { useUiText } from "../../i18n/index.ts";
import { frameworkFilterLabel, frameworkIdentity } from "../../services/artHubUi";
import type { LoomFramework } from "../../services/loomApi";

export function FrameworkFilter({
  frameworks,
  selectedFrameworkIds,
  onToggle,
  actions,
}: {
  frameworks: LoomFramework[];
  selectedFrameworkIds: ReadonlySet<string> | null;
  onToggle: (frameworkId: string) => void;
  actions: ReactNode;
}) {
  const { t, tf } = useUiText();
  const options = frameworks.map((framework) => ({
    value: frameworkIdentity(framework),
    label: frameworkFilterLabel(framework),
  }));
  const selected = options
    .filter((option) => selectedFrameworkIds === null || selectedFrameworkIds.has(option.value))
    .map((option) => option.value);
  return (
    <Toolbar className="framework-filter" role="group" aria-label={t("art.filter.ariaLabel")}>
      <div className="framework-filter__options">
        {frameworks.length > 6 ? (
          <MultiSelect
            label={t("art.filter.frameworkLabel")}
            compact
            className="loom-art-framework-picker"
            options={options}
            values={selected}
            formatSummary={(count) => tf("art.filter.selectedSummary", { count })}
            search={{ label: t("art.filter.searchLabel"), emptyMessage: t("art.filter.emptyMatch") }}
            onValuesChange={(values) => {
              for (const option of options)
                if (selected.includes(option.value) !== values.includes(option.value)) onToggle(option.value);
            }}
          />
        ) : (
          options.map((option) => (
            <Checkbox
              key={option.value}
              label={option.label}
              checked={selected.includes(option.value)}
              onChange={() => onToggle(option.value)}
            />
          ))
        )}
        {frameworks.length === 0 ? <span className="n-muted">{t("art.framework.empty")}</span> : null}
      </div>
      <div className="framework-filter__actions">{actions}</div>
    </Toolbar>
  );
}
