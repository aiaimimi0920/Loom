// Framework identity and filter selection remain application-owned.
import { Checkbox, MultiSelect, Toolbar } from "@neuro/ui";
import type { ReactNode } from "react";
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
  const options = frameworks.map((framework) => ({
    value: frameworkIdentity(framework),
    label: frameworkFilterLabel(framework),
  }));
  const selected = options
    .filter((option) => selectedFrameworkIds === null || selectedFrameworkIds.has(option.value))
    .map((option) => option.value);
  return (
    <Toolbar className="framework-filter" role="group" aria-label="按框架筛选 Art">
      <div className="framework-filter__options">
        {frameworks.length > 6 ? (
          <MultiSelect
            label="框架"
            compact
            className="loom-art-framework-picker"
            options={options}
            values={selected}
            formatSummary={(count) => `已选 ${count} 个框架`}
            search={{ label: "搜索框架", emptyMessage: "没有匹配的框架" }}
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
        {frameworks.length === 0 ? <span className="n-muted">暂无框架</span> : null}
      </div>
      <div className="framework-filter__actions">{actions}</div>
    </Toolbar>
  );
}
