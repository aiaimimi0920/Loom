import { NeuroProvider, type NeuroMode } from "@neuro/ui";
import { useSyncExternalStore, type ReactNode } from "react";

const LIGHT_QUERY = "(prefers-color-scheme: light)";

function readMode(): NeuroMode {
  const mode = document.documentElement.dataset.loomTheme;
  return mode === "light" || (mode === "system" && window.matchMedia?.(LIGHT_QUERY).matches) ? "light" : "dark";
}

function subscribe(onChange: () => void) {
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-loom-theme"] });
  const query = window.matchMedia?.(LIGHT_QUERY);
  query?.addEventListener("change", onChange);
  return () => { observer.disconnect(); query?.removeEventListener("change", onChange); };
}

/** Settings persistence stays in Loom; only the resolved presentation mode is shared. */
export function LoomUiProvider({ children }: { children: ReactNode }) {
  const mode = useSyncExternalStore(subscribe, readMode, (): NeuroMode => "dark");
  return <NeuroProvider mode={mode} inheritTypography style={{ height: "100%", minHeight: 0 }}>{children}</NeuroProvider>;
}
