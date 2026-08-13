type HookBridgeHandler = (payload: unknown) => void;

type HookBridgeSocketLike = {
  readonly readyState: number;
  onopen: ((event?: unknown) => void) | null;
  onmessage: ((event: { data: string }) => void) | null;
  onclose: ((event?: unknown) => void) | null;
  onerror: ((event?: unknown) => void) | null;
  send(payload: string): void;
  close(): void;
};

type HookBridgeWebSocketFactory = (url: string) => HookBridgeSocketLike;

interface HookBridgeBrowserClientOptions {
  url?: string;
  websocketFactory?: HookBridgeWebSocketFactory;
  reconnectDelayMs?: number;
  scheduleReconnect?: (callback: () => void, delayMs: number) => number;
  cancelReconnect?: (handle: number) => void;
  logger?: Pick<Console, "error">;
}

export interface HookBridgeBrowserClient {
  subscribe(event: string, handler: HookBridgeHandler): () => void;
  dispose(): void;
}

const DEFAULT_HOOK_BRIDGE_URL = "ws://127.0.0.1:19820";
const DEFAULT_RECONNECT_DELAY_MS = 1000;
const SOCKET_OPEN = 1;
const SOCKET_CLOSED = 3;

const socketIsClosed = (socket: HookBridgeSocketLike | null) =>
  !socket || socket.readyState === SOCKET_CLOSED;

export function createHookBridgeBrowserClient(
  options: HookBridgeBrowserClientOptions = {},
): HookBridgeBrowserClient {
  const hookBridgeUrl = options.url ?? DEFAULT_HOOK_BRIDGE_URL;
  const websocketFactory =
    options.websocketFactory ??
    ((url: string) => new WebSocket(url) as unknown as HookBridgeSocketLike);
  const reconnectDelayMs = options.reconnectDelayMs ?? DEFAULT_RECONNECT_DELAY_MS;
  const scheduleReconnect =
    options.scheduleReconnect ??
    ((callback: () => void, delayMs: number) => window.setTimeout(callback, delayMs));
  const cancelReconnect =
    options.cancelReconnect ?? ((handle: number) => window.clearTimeout(handle));
  const logger = options.logger ?? console;

  const handlers = new Map<string, Set<HookBridgeHandler>>();
  let socket: HookBridgeSocketLike | null = null;
  let reconnectHandle: number | null = null;
  let disposed = false;

  const clearReconnect = () => {
    if (reconnectHandle === null) return;
    cancelReconnect(reconnectHandle);
    reconnectHandle = null;
  };

  const sendSubscription = () => {
    if (!socket || socket.readyState !== SOCKET_OPEN) return;
    const events = Array.from(handlers.keys());
    if (events.length === 0) return;
    socket.send(
      JSON.stringify({
        method: "loom.hook.subscribe",
        params: {
          requestId: `subscribe:${globalThis.crypto?.randomUUID?.() ?? Date.now()}`,
          events,
        },
      }),
    );
  };

  const scheduleReconnectIfNeeded = () => {
    if (disposed || reconnectHandle !== null || handlers.size === 0) return;
    reconnectHandle = scheduleReconnect(() => {
      reconnectHandle = null;
      ensureSocket();
    }, reconnectDelayMs);
  };

  const stopSocketIfUnused = () => {
    if (handlers.size > 0) return;
    clearReconnect();
    if (!socket) return;
    const current = socket;
    socket = null;
    current.close();
  };

  const ensureSocket = () => {
    if (disposed || handlers.size === 0) return;
    if (socket && socket.readyState !== SOCKET_CLOSED) {
      if (socket.readyState === SOCKET_OPEN) {
        sendSubscription();
      }
      return;
    }

    const nextSocket = websocketFactory(hookBridgeUrl);
    socket = nextSocket;

    nextSocket.onopen = () => {
      clearReconnect();
      sendSubscription();
    };

    nextSocket.onmessage = (event) => {
      try {
        const parsed = JSON.parse(String(event.data));
        const method = typeof parsed?.method === "string" ? parsed.method : null;
        if (!method) return;

        const channelHandlers = handlers.get(method);
        if (!channelHandlers || channelHandlers.size === 0) return;
        channelHandlers.forEach((handler) => handler(parsed.params));
      } catch (error) {
        logger.error("[hookBridgeBrowserClient] Failed to process bridge payload:", error);
      }
    };

    nextSocket.onclose = () => {
      if (socket === nextSocket) {
        socket = null;
      }
      scheduleReconnectIfNeeded();
    };

    nextSocket.onerror = () => {
      nextSocket.close();
    };
  };

  return {
    subscribe(channel: string, handler: HookBridgeHandler) {
      const normalized = channel.trim();
      if (!normalized) {
        return () => undefined;
      }

      const channelHandlers = handlers.get(normalized) ?? new Set<HookBridgeHandler>();
      channelHandlers.add(handler);
      handlers.set(normalized, channelHandlers);
      ensureSocket();
      if (socket && socket.readyState === SOCKET_OPEN) {
        sendSubscription();
      }

      return () => {
        const existing = handlers.get(normalized);
        if (!existing) return;
        existing.delete(handler);
        if (existing.size === 0) {
          handlers.delete(normalized);
        }
        stopSocketIfUnused();
      };
    },
    dispose() {
      disposed = true;
      handlers.clear();
      clearReconnect();
      if (!socketIsClosed(socket)) {
        socket?.close();
      }
      socket = null;
    },
  };
}
