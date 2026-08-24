// Stable desktop API facade. Domain clients depend on the shared transport, never on this file.
export * from "./loomApi/coreTypes.ts";
export * from "./loomApi/defaults.ts";
export * from "./loomApi/hookTypes.ts";
export * from "./loomApi/mcpTypes.ts";
export * from "./loomApi/pythonTypes.ts";
export * from "./loomApi/settingsTypes.ts";
export * from "./loomApi/snapshotTypes.ts";

export { getLoomDaemonJson, loadHookCanvasPreview, startLoomDaemon } from "./loomApi/transport.ts";
export * from "./loomApi/snapshot.ts";
export * from "./loomApi/hook.ts";
export * from "./loomApi/frameworks.ts";
export * from "./loomApi/plugins.ts";
export * from "./loomApi/arts.ts";
export * from "./loomApi/workflows.ts";
export * from "./loomApi/mcp.ts";
export * from "./loomApi/runtime.ts";
