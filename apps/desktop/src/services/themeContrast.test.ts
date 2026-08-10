import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const styleSource = readFileSync(new URL("../styles.css", import.meta.url), "utf8");
const appSource = readFileSync(new URL("../App.tsx", import.meta.url), "utf8");

type Rgba = readonly [number, number, number, number];

const extractTokenBlock = (selector: string): Record<string, string> => {
  const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = styleSource.match(new RegExp(`${escapedSelector}\\s*\\{([\\s\\S]*?)\\n\\}`));
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

const extractThemeTokens = (selector: string): Record<string, string> => {
  const rootTokens = extractTokenBlock(":root");
  const allTokens = selector === ":root"
    ? rootTokens
    : { ...rootTokens, ...extractTokenBlock(selector) };

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
    return normalized <= 0.04045
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
};

const contrastRatio = (foregroundValue: string, backgroundValue: string): number => {
  const background = parseColor(backgroundValue);
  const foreground = composite(parseColor(foregroundValue), background);
  const foregroundLuminance = luminance(foreground);
  const backgroundLuminance = luminance(background);
  return (Math.max(foregroundLuminance, backgroundLuminance) + 0.05) /
    (Math.min(foregroundLuminance, backgroundLuminance) + 0.05);
};

const assertTokenContrast = (
  theme: string,
  tokens: Record<string, string>,
  foreground: string,
  background: string,
  minimum: number,
) => {
  const ratio = contrastRatio(tokens[foreground], tokens[background]);
  assert.ok(
    ratio >= minimum,
    `${theme} ${foreground}/${background} contrast ${ratio.toFixed(2)} is below ${minimum}`,
  );
};

test("keeps text, status, icon, and brand colors visible in dark and light themes", () => {
  const themes = {
    dark: extractThemeTokens(":root"),
    light: extractThemeTokens(':root[data-loom-theme="light"]'),
  };

  for (const [theme, tokens] of Object.entries(themes)) {
    for (const background of ["bg", "surface", "panel", "control", "control-hover"]) {
      assertTokenContrast(theme, tokens, "text", background, 4.5);
      assertTokenContrast(theme, tokens, "muted", background, 4.5);
    }
    for (const foreground of [
      "accent-text",
      "secondary-text",
      "success",
      "warning",
      "danger",
    ]) {
      assertTokenContrast(theme, tokens, foreground, "panel", 4.5);
    }
    assertTokenContrast(theme, tokens, "brand-primary", "rail", 3);
    assertTokenContrast(theme, tokens, "brand-secondary", "rail", 3);
    assertTokenContrast(theme, tokens, "bg", "danger", 4.5);
  }
});

test("system light mode reuses the complete accessible light palette", () => {
  const light = extractThemeTokens(':root[data-loom-theme="light"]');
  const system = extractThemeTokens(':root[data-loom-theme="system"]');
  for (const token of [
    "bg",
    "surface",
    "panel",
    "rail",
    "control",
    "control-hover",
    "text",
    "muted",
    "accent-text",
    "secondary-text",
    "success",
    "warning",
    "danger",
    "brand-primary",
    "brand-secondary",
  ]) {
    assert.equal(system[token], light[token], `system light token ${token} drifted from light`);
  }
});

test("functional graphics and product marks use theme-aware colors", () => {
  assert.match(appSource, /fill="var\(--loom-brand-primary\)"/);
  assert.match(appSource, /fill="var\(--loom-brand-secondary\)"/);
  assert.match(appSource, /stroke="var\(--loom-brand-primary\)"/);
  assert.match(appSource, /stroke="var\(--loom-brand-secondary\)"/);
  assert.match(styleSource, /\.rail-item__icon \{[\s\S]*?color: var\(--loom-theme-muted\);/);
  assert.match(styleSource, /\.settings-section__icon \{[\s\S]*?color: var\(--loom-theme-accent-text\);/);
});

test("shared application surfaces consume semantic theme aliases", () => {
  assert.match(styleSource, /\.app-toast \{[\s\S]*?background: var\(--loom-theme-panel\);/);
  assert.match(styleSource, /\.app-confirm-dialog \{[\s\S]*?background: var\(--loom-theme-panel\);/);
  assert.match(styleSource, /\.framework-dialog \{[\s\S]*?background: var\(--loom-theme-panel\);/);
  assert.match(styleSource, /\.studio-hero \{[\s\S]*?background: var\(--loom-theme-panel\);/);
  assert.match(styleSource, /\.workflow-live-card \{[\s\S]*?var\(--loom-theme-success\)/);
  assert.doesNotMatch(styleSource, /\.workflow-live-card \{[\s\S]*?107,\s*93,\s*255/);
});

test("routes every theme-responsive workspace and control state through semantic colors", () => {
  assert.match(styleSource, /\.workspace-panel,[\s\S]*?background: var\(--loom-theme-surface\);[\s\S]*?color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.ghost-button,[\s\S]*?background: var\(--loom-theme-control\);[\s\S]*?color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /:is\(\.studio-input, \.studio-textarea, \.studio-json\):read-only \{[\s\S]*?background: var\(--loom-theme-control-hover\);[\s\S]*?-webkit-text-fill-color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /:is\(\.studio-input, \.studio-textarea, \.studio-json\):disabled \{[\s\S]*?color: var\(--loom-theme-muted\);[\s\S]*?opacity: 1;/);
  assert.match(styleSource, /\.app-toast \{[\s\S]*?background: var\(--loom-theme-panel\);[\s\S]*?color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.app-confirm-dialog \{[\s\S]*?background: var\(--loom-theme-panel\);[\s\S]*?color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.device-management,[\s\S]*?background: var\(--loom-theme-surface\);[\s\S]*?color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.device-card,[\s\S]*?background: var\(--loom-theme-panel\);[\s\S]*?color: var\(--loom-theme-text\);/);
  assert.match(styleSource, /\.settings-page \.studio-input:disabled,[\s\S]*?-webkit-text-fill-color: var\(--loom-theme-muted\);[\s\S]*?opacity: 1;/);
});
