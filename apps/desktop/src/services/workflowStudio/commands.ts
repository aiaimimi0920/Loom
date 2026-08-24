import {
  MAX_COMMAND_TOKENS,
  MAX_IMPORT_TEXT_CHARS,
  defineOwnValue,
  stripQuotes,
} from "./shared.ts";
import { parseTemplate, templateObjectInputs } from "./templates.ts";
import type { CurlImportResult, ParsedPort, RawCommandImportResult } from "./types.ts";

const commandWhitespace = /\s/;

const tokenizeCommand = (command: string): string[] | null => {
  const tokens: string[] = [];
  let current = "";
  let quote: string | null = null;

  for (let index = 0; index < command.length; index += 1) {
    const char = command[index];
    if ((char === '"' || char === "'") && command[index - 1] !== "\\") {
      if (quote === char) quote = null;
      else if (!quote) quote = char;
      else current += char;
      continue;
    }

    if (commandWhitespace.test(char) && !quote) {
      if (current) {
        tokens.push(current);
        if (tokens.length > MAX_COMMAND_TOKENS) return null;
        current = "";
      }
      continue;
    }
    current += char;
  }

  if (current) tokens.push(current);
  return tokens.length > MAX_COMMAND_TOKENS ? null : tokens;
};

export function parseCurlCommand(curlCommand: string): CurlImportResult | null {
  if (curlCommand.length > MAX_IMPORT_TEXT_CHARS || !/^curl(?:\s|$)/i.test(curlCommand.trim())) return null;
  const tokens = tokenizeCommand(curlCommand);
  if (!tokens) return null;

  let url = "";
  let method = "GET";
  const headers: Record<string, string> = {};
  let body = "";

  for (let index = 1; index < tokens.length; index += 1) {
    const token = tokens[index];
    const next = tokens[index + 1] ?? "";
    if (token === "-X" || token === "--request") {
      method = stripQuotes(next).toUpperCase();
      index += 1;
    } else if (token === "-H" || token === "--header") {
      const [key, ...rest] = stripQuotes(next).split(":");
      if (key) defineOwnValue(headers, key.trim(), rest.join(":").trim());
      index += 1;
    } else if (["--data", "-d", "--data-raw", "--data-binary"].includes(token)) {
      body = stripQuotes(next);
      if (method === "GET") method = "POST";
      index += 1;
    } else if (/^https?:\/\//i.test(token)) {
      url = stripQuotes(token);
    }
  }

  const suggestedInputs: ParsedPort[] = [];
  if (body) {
    try {
      const parsed = JSON.parse(body) as unknown;
      const templated = templateObjectInputs(parsed, suggestedInputs);
      body = JSON.stringify(templated, null, 2);
    } catch {
      suggestedInputs.splice(0);
      suggestedInputs.push(...parseTemplate(body).filter((port) => port.isInput));
    }
  }

  return { url, method, headers, body, suggestedInputs };
}

export function parseRawCommand(rawCommand: string): RawCommandImportResult | null {
  if (rawCommand.length > MAX_IMPORT_TEXT_CHARS) return null;
  const tokens = tokenizeCommand(rawCommand.trim());
  if (!tokens?.length) return null;

  const [command, ...args] = tokens;
  return {
    command,
    args,
    argsText: args.join("\n"),
    ports: parseTemplate(args.join(" ")),
  };
}
