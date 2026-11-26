import { defineConfig } from "@rsbuild/core";
import { pluginReact } from "@rsbuild/plugin-react";

const isWebTarget = process.env.LOOM_UI_TARGET === "web";

export default defineConfig({
  plugins: [pluginReact()],
  source: {
    entry: {
      index: "./src/main.tsx",
    },
    define: {
      __LOOM_UI_TARGET__: JSON.stringify(isWebTarget ? "web" : "tauri"),
    },
  },
  output: isWebTarget
    ? {
        assetPrefix: "/ui/",
        distPath: {
          root: "dist/web",
        },
      }
    : undefined,
  html: {
    title: "Loom",
    mountId: "root",
  },
  server: {
    port: 1423,
    strictPort: true,
  },
});
