// Bundled interface catalogs with English source contracts and deterministic fallback.

import { type LocaleShard, mergeLocaleShards, type ParsedTemplate } from "./contract.ts";
import { type DisplayLanguage } from "./displayLanguage.ts";

import { enArt } from "../locales/en/art.ts";
import { enCommon } from "../locales/en/common.ts";
import { enDevices } from "../locales/en/devices.ts";
import { enMcp } from "../locales/en/mcp.ts";
import { enOtter } from "../locales/en/otter.ts";
import { enSecurity } from "../locales/en/security.ts";
import { enSettings } from "../locales/en/settings.ts";
import { enShell } from "../locales/en/shell.ts";

import { zhHansArt } from "../locales/zh-Hans/art.ts";
import { zhHansCommon } from "../locales/zh-Hans/common.ts";
import { zhHansDevices } from "../locales/zh-Hans/devices.ts";
import { zhHansMcp } from "../locales/zh-Hans/mcp.ts";
import { zhHansOtter } from "../locales/zh-Hans/otter.ts";
import { zhHansSecurity } from "../locales/zh-Hans/security.ts";
import { zhHansSettings } from "../locales/zh-Hans/settings.ts";
import { zhHansShell } from "../locales/zh-Hans/shell.ts";

export type MessageArguments = Readonly<Record<string, string | number>>;

class LanguageCatalog {
  readonly templates: Map<string, ParsedTemplate>;
  readonly labels: Map<string, string>;

  constructor(language: DisplayLanguage, shards: readonly LocaleShard[]) {
    this.templates = mergeLocaleShards(language, shards);
    this.labels = new Map<string, string>();
    for (const [id, parsed] of this.templates.entries()) {
      if (parsed.parameters.size === 0) {
        this.labels.set(id, parsed.segments.map((segment) => (segment.kind === "text" ? segment.value : "")).join(""));
      }
    }
  }

  resolve(id: string, args: MessageArguments): string | null {
    const parsed = this.templates.get(id);
    if (!parsed) {
      return null;
    }
    let output = "";
    for (const segment of parsed.segments) {
      if (segment.kind === "text") {
        output += segment.value;
      } else {
        output += String(args[segment.name] ?? "");
      }
    }
    return output;
  }
}

export class Catalogs {
  private readonly english: LanguageCatalog;
  private readonly chinese: LanguageCatalog;

  constructor(englishShards: readonly LocaleShard[], chineseShards: readonly LocaleShard[]) {
    this.english = new LanguageCatalog("en", englishShards);
    this.chinese = new LanguageCatalog("zh-Hans", chineseShards);

    for (const [id, translated] of this.chinese.templates.entries()) {
      const source = this.english.templates.get(id);
      if (!source) {
        throw new Error(`Translated interface message has no English source: "${id}"`);
      }
      for (const param of translated.parameters) {
        if (!source.parameters.has(param)) {
          throw new Error(`Translated interface message "${id}" requires undeclared parameter "${param}"`);
        }
      }
    }
  }

  static embedded(): Catalogs {
    return new Catalogs(
      [enCommon, enShell, enArt, enMcp, enDevices, enOtter, enSecurity, enSettings],
      [zhHansCommon, zhHansShell, zhHansArt, zhHansMcp, zhHansDevices, zhHansOtter, zhHansSecurity, zhHansSettings],
    );
  }

  private localized(language: DisplayLanguage): LanguageCatalog {
    return language === "zh-Hans" ? this.chinese : this.english;
  }

  text(language: DisplayLanguage, id: string): string {
    const source = this.english.labels.get(id);
    if (source === undefined) {
      throw new Error(`Unknown or parameterized interface message: "${id}"`);
    }
    return this.localized(language).labels.get(id) ?? source;
  }

  format(language: DisplayLanguage, id: string, values: MessageArguments): string {
    const source = this.english.templates.get(id);
    if (!source) {
      throw new Error(`Unknown interface message: "${id}"`);
    }
    const suppliedKeys = Object.keys(values);
    if (suppliedKeys.length !== source.parameters.size) {
      throw new Error(`Incorrect arguments for interface message "${id}"`);
    }
    for (const key of suppliedKeys) {
      if (!source.parameters.has(key)) {
        throw new Error(`Incorrect arguments for interface message "${id}": unexpected "${key}"`);
      }
    }
    return this.localized(language).resolve(id, values) ?? this.english.resolve(id, values)!;
  }

  sourceParameters(id: string): ReadonlySet<string> | undefined {
    return this.english.templates.get(id)?.parameters;
  }

  validateCompleteness(): void {
    const englishKeys = [...this.english.templates.keys()].sort();
    const chineseKeys = [...this.chinese.templates.keys()].sort();
    const missingInChinese = englishKeys.filter((key) => !this.chinese.templates.has(key));
    if (missingInChinese.length > 0 || englishKeys.length !== chineseKeys.length) {
      throw new Error(`Bundled Simplified Chinese catalog is incomplete. Missing keys: ${missingInChinese.join(", ")}`);
    }
    for (const [id, parsed] of this.english.templates.entries()) {
      const sampleArgs: Record<string, number> = {};
      for (const param of parsed.parameters) {
        sampleArgs[param] = 2;
      }
      if (this.english.resolve(id, sampleArgs) === null || this.chinese.resolve(id, sampleArgs) === null) {
        throw new Error(`Failed to resolve interface message "${id}"`);
      }
    }
  }
}
