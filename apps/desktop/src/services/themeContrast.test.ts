import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { desktopStyleSource as styleSource } from "./desktopStyleSource.ts";
const uiStyles = readFileSync(new URL(import.meta.resolve("@neuro/ui/styles.css")), "utf8");

const appSource = ["../components/app/appShell.tsx", "../components/settings/AboutPanel.tsx"]
  .map((relativePath) => readFileSync(new URL(relativePath, import.meta.url), "utf8"))
  .join("\n");

type Rgba = readonly [number, number, number, number];

const extractTokenBlock = (selector: string, source = styleSource): Record<string, string> => {
  const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = source.match(new RegExp(`${escapedSelector}\\s*\\{([\\s\\S]*?)\\n\\}`));
  assert.ok(match, `missing theme block: ${selector}`);

  return Object.fromEntries(
    [...match[1].matchAll(/--([\w-]+):\s*([^;]+);/g)].map((entry) => [entry[1], entry[2].trim()]),
  );
};

const colorToCss = (color: Rgba): string =>
  `rgba(${Math.round(color[0])}, ${Math.round(color[1])}, ${Math.round(color[2])}, ${color[3]})`;

const resolveCssColor = (
  value: string,
  tokens: Record<string, string>,
  resolving: ReadonlySet<string> = new Set(),
): Rgba => {
  const normalized = value.trim();
  if (normalized === "white") return [255, 255, 255, 1];
  if (normalized === "black") return [0, 0, 0, 1];
  if (normalized === "transparent") return [0, 0, 0, 0];

  const variable = normalized.match(/^var\(--([\w-]+)\)$/)?.[1];
  if (variable) {
    assert.ok(!resolving.has(variable), `circular theme token: ${variable}`);
    assert.ok(tokens[variable], `missing referenced theme token: ${variable}`);
    return resolveCssColor(tokens[variable], tokens, new Set([...resolving, variable]));
  }

  const mix = normalized.match(/^color-mix\(in srgb,\s*(.+?)\s+([\d.]+)%,\s*(.+?)\)$/);
  if (mix) {
    const first = resolveCssColor(mix[1], tokens, resolving);
    const second = resolveCssColor(mix[3], tokens, resolving);
    const firstWeight = Number(mix[2]) / 100;
    const secondWeight = 1 - firstWeight;
    const alpha = first[3] * firstWeight + second[3] * secondWeight;
    if (alpha === 0) return [0, 0, 0, 0];
    return [
      (first[0] * first[3] * firstWeight + second[0] * second[3] * secondWeight) / alpha,
      (first[1] * first[3] * firstWeight + second[1] * second[3] * secondWeight) / alpha,
      (first[2] * first[3] * firstWeight + second[2] * second[3] * secondWeight) / alpha,
      alpha,
    ];
  }

  return parseColor(normalized);
};

const extractThemeTokens = (mode: "dark" | "light"): Record<string, string> => {
  const allTokens = {
    ...extractTokenBlock(".neuro-root", uiStyles),
    ...(mode === "light" ? extractTokenBlock('.neuro-root[data-neuro-mode="light"]', uiStyles) : {}),
    ...extractTokenBlock(".neuro-root"),
  };

  return Object.fromEntries(
    Object.keys(allTokens)
      .filter((name) => name.startsWith("loom-theme-") || name.startsWith("loom-brand-"))
      .map((name) => [
        name.replace(/^loom-theme-/, "").replace(/^loom-brand-/, "brand-"),
        colorToCss(resolveCssColor(allTokens[name], allTokens, new Set([name]))),
      ]),
  );
};

const parseColor = (value: string): Rgba => {
  const hex = value.match(/^#([\da-f]{6})$/i)?.[1];
  if (hex) {
    return [
      Number.parseInt(hex.slice(0, 2), 16),
      Number.parseInt(hex.slice(2, 4), 16),
      Number.parseInt(hex.slice(4, 6), 16),
      1,
    ];
  }

  const rgba = value.match(/^rgba\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*([\d.]+)\s*\)$/i);
  assert.ok(rgba, `unsupported theme color: ${value}`);
  return [Number(rgba[1]), Number(rgba[2]), Number(rgba[3]), Number(rgba[4])];
};

const composite = (foreground: Rgba, background: Rgba): Rgba => {
  const alpha = foreground[3] + background[3] * (1 - foreground[3]);
  return [
    (foreground[0] * foreground[3] + background[0] * background[3] * (1 - foreground[3])) / alpha,
    (foreground[1] * foreground[3] + background[1] * background[3] * (1 - foreground[3])) / alpha,
    (foreground[2] * foreground[3] + background[2] * background[3] * (1 - foreground[3])) / alpha,
    alpha,
  ];
};

const luminance = (color: Rgba): number => {
  const channels = color.slice(0, 3).map((channel) => {
    const normalized = channel / 255;
    return normalized <= 0.04045 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
};

const contrastRatio = (foregroundValue: string, backgroundValue: string): number => {
  const background = parseColor(backgroundValue);
  const foreground = composite(parseColor(foregroundValue), background);
  const foregroundLuminance = luminance(foreground);
  const backgroundLuminance = luminance(background);
  return (
    (Math.max(foregroundLuminance, backgroundLuminance) + 0.05) /
    (Math.min(foregroundLuminance, backgroundLuminance) + 0.05)
  );
};

const assertTokenContrast = (
  theme: string,
  tokens: Record<string, string>,
  foreground: string,
  background: string,
  minimum: number,
) => {
  const ratio = contrastRatio(tokens[foreground], tokens[background]);
  assert.ok(ratio >= minimum, `${theme} ${foreground}/${background} contrast ${ratio.toFixed(2)} is below ${minimum}`);
};

test("keeps text, status, icon, and brand colors visible in dark and light themes", () => {
  const themes = {
    dark: extractThemeTokens("dark"),
    light: extractThemeTokens("light"),
  };

  for (const [theme, tokens] of Object.entries(themes)) {
    for (const background of ["bg", "surface", "panel", "control", "control-hover"]) {
      assertTokenContrast(theme, tokens, "text", background, 4.5);
      assertTokenContrast(theme, tokens, "muted", background, 4.5);
    }
    for (const foreground of ["accent-text", "secondary-text", "success", "warning", "danger"]) {
      assertTokenContrast(theme, tokens, foreground, "panel", 4.5);
    }
    assertTokenContrast(theme, tokens, "brand-primary", "rail", 3);
    assertTokenContrast(theme, tokens, "brand-secondary", "rail", 3);
    assertTokenContrast(theme, tokens, "bg", "danger", 4.5);
  }
});

test("system mode resolves through the same Provider instead of copying a palette", () => {
  const provider = readFileSync(new URL("../components/LoomUiProvider.tsx", import.meta.url), "utf8");
  assert.match(provider, /mode === "light" \|\| \(mode === "system" && window\.matchMedia/);
  assert.match(provider, /<NeuroProvider mode=\{mode\}/);
  assert.doesNotMatch(styleSource, /--neuro-|--n-[\w-]+\s*:/);
  for (const name of Object.keys(extractTokenBlock(".neuro-root"))) {
    assert.equal((styleSource.match(new RegExp(`--${name}:`, "g")) || []).length, 1, `duplicate alias ${name}`);
  }
});

test("functional graphics and product marks use theme-aware colors", () => {
  assert.match(appSource, /fill="var\(--loom-brand-primary\)"/);
  assert.match(appSource, /fill="var\(--loom-brand-secondary\)"/);
  assert.match(appSource, /stroke="var\(--loom-brand-primary\)"/);
  assert.match(appSource, /stroke="var\(--loom-brand-secondary\)"/);
  assert.match(styleSource, /\.rail-item__icon \{[^}]*color: inherit;/);
  assert.match(styleSource, /\.settings-section__icon \{[\s\S]*?color: var\(--loom-theme-accent-text\);/);
});

test("shared application surfaces consume semantic theme aliases", () => {
  for (const component of ["n-toast", "n-dialog", "n-panel"]) {
    assert.match(uiStyles, new RegExp(`\\.${component}\\s*\\{[^}]*background: var\\(--n-panel\\);`));
    assert.match(uiStyles, new RegExp(`\\.${component}\\s*\\{[^}]*border: 1px solid var\\(--n-line\\);`));
  }
  const feedback = readFileSync(new URL("../components/feedback/AppFeedback.tsx", import.meta.url), "utf8");
  assert.match(feedback, /<ToastViewport entries=\{notifications\}/);
  assert.match(feedback, /<ConfirmDialog open title=\{active\.title\}/);
  assert.doesNotMatch(styleSource, /\.app-(?:toast|confirm-)|\.framework-dialog(?:\s|\{|:)/);
});

test("routes every theme-responsive workspace and control state through semantic colors", () => {
  assert.match(
    styleSource,
    /\.workspace-panel,[\s\S]*?background: var\(--loom-theme-surface\);[\s\S]*?color: var\(--loom-theme-text\);/,
  );
  assert.match(uiStyles, /\.n-button:disabled\s*\{[^}]*background: var\(--n-control\);[^}]*color: var\(--n-muted\);/);
  assert.match(
    uiStyles,
    /input\.n-input:read-only, textarea\.n-input:read-only\s*\{[^}]*background: var\(--n-control-hover\);/,
  );
  assert.match(uiStyles, /\.n-input:disabled\s*\{[^}]*color: var\(--n-muted\);[^}]*opacity: 1;/);
  assert.match(uiStyles, /\.n-toast\s*\{[^}]*background: var\(--n-panel\);[^}]*color: var\(--n-text\);/);
  assert.match(uiStyles, /\.n-dialog\s*\{[^}]*color: var\(--n-text\);[^}]*background: var\(--n-panel\);/);
  assert.match(
    styleSource,
    /\.workspace-scroll--devices \{[\s\S]*?background: var\(--loom-theme-surface\);[\s\S]*?color: var\(--loom-theme-text\);/,
  );
  assert.doesNotMatch(styleSource, /\.device-(?:card|add-dialog|edit-dialog|request-row)(?:[\s,:.#[]|\{)/);
  assert.match(uiStyles, /\.neuro-root\[data-neuro-mode="light"\]\s*\{[^}]*color-scheme: light;/);
});
