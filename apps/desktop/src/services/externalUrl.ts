// Keeps renderer-triggered external navigation on explicit HTTPS URLs.
import { uiText } from "../i18n/index.ts";

export function normalizeHttpsExternalUrl(value: string): string {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error(uiText("security.externalUrl.invalidFormat"));
  }
  if (url.protocol !== "https:" || url.username || url.password) {
    throw new Error(uiText("security.externalUrl.httpsWithoutCredentialsRequired"));
  }
  return url.toString();
}
