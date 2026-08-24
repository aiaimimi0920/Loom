// Resolves the ordered desktop CSS import graph for source-contract tests.
import { readFileSync } from "node:fs";

const styleEntryUrl = new URL("../styles.css", import.meta.url);

export const desktopStyleEntrySource = readFileSync(styleEntryUrl, "utf8");

const declaredImports = [
  ...desktopStyleEntrySource.matchAll(/^@import\s+"([^"]+)";\s*$/gm),
].map((match) => match[1]);

const uniqueImports = new Set(declaredImports);
if (uniqueImports.size !== declaredImports.length) {
  throw new Error("Desktop stylesheet entry contains duplicate imports");
}

export const desktopStyleModules = declaredImports.map((relativePath) => {
  if (!/^\.\/styles\/[a-z0-9-]+\.css$/.test(relativePath)) {
    throw new Error(`Desktop stylesheet import is not an owned local module: ${relativePath}`);
  }
  const source = readFileSync(new URL(relativePath, styleEntryUrl), "utf8");
  return { relativePath, source } as const;
});

export const desktopStyleSource = desktopStyleModules
  .map(({ source }) => source)
  .join("\n");
