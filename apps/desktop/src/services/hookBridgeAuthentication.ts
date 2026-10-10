import { invoke, isTauri } from "@tauri-apps/api/core";

// Standalone development previews have no ambient access to the local manifest.
let previewToken: string | null = null;
export function setHookBridgePreviewToken(token: string | null): void {
  if (token !== null && (!token || token.length > 4096)) throw new Error("Invalid local credential");
  previewToken = token;
}

export async function createAuthenticatedHookBridgeSocket(endpoint: string): Promise<WebSocket> {
  const url = new URL(endpoint);
  if (url.protocol !== "ws:" || !["localhost", "127.0.0.1", "[::1]"].includes(url.hostname)
    || url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
    throw new Error("Hook authentication requires a credential-free loopback endpoint");
  }
  let protocols: string[];
  if (isTauri()) {
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      protocols = await Promise.race([
        invoke<string[]>("hook_bridge_websocket_protocols", { endpoint }),
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => reject(new Error("Local authentication timed out")), 5000);
        }),
      ]);
    } finally {
      if (timer !== undefined) clearTimeout(timer);
    }
  } else {
    if (!previewToken) throw new Error("Local Hook authentication is required");
    const encoded = btoa(Array.from(new TextEncoder().encode(previewToken), (byte) => String.fromCharCode(byte)).join(""))
      .replace(/\+/gu, "-").replace(/\//gu, "_").replace(/=+$/u, "");
    protocols = ["loom.hook.v1", `loom.auth.${encoded}`];
  }
  return new WebSocket(endpoint, protocols);
}
