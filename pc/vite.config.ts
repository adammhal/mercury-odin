import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

const here = (p: string) => fileURLToPath(new URL(p, import.meta.url));

// The PC app reuses the Odin plugin's API client, hooks and styles from ../plugin/src.
// Those import "@decky/ui" and "@decky/api"; here they resolve to small Windows stand-ins.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  resolve: {
    alias: {
      "@decky/ui": here("./src/shim/ui.tsx"),
      "@decky/api": here("./src/shim/api.ts"),
      "@shared": here("../plugin/src"),
    },
    dedupe: ["react", "react-dom", "react-icons"],
  },
  build: { target: "chrome110", outDir: "dist", emptyOutDir: true },
});
