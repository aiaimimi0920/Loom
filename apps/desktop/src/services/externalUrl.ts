// Keeps renderer-triggered external navigation on explicit HTTPS URLs.
export function normalizeHttpsExternalUrl(value: string): string {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error("外部链接格式无效。");
  }
  if (url.protocol !== "https:" || url.username || url.password) {
    throw new Error("仅允许打开不含凭据的 HTTPS 外部链接。");
  }
  return url.toString();
}
