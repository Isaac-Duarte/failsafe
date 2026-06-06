import path from "path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig, type Plugin } from "vite"

const host = process.env.TAURI_DEV_HOST
const uiRoot = path.resolve(__dirname, "../failsafe-ui/src")

function uiPackageAlias(): Plugin {
  return {
    name: "ui-package-alias",
    resolveId(source, importer) {
      if (source.startsWith("@/") && importer?.includes(`${path.sep}failsafe-ui${path.sep}`)) {
        return path.join(uiRoot, source.slice(2))
      }
      return null
    },
  }
}

export default defineConfig({
  plugins: [react(), tailwindcss(), uiPackageAlias()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@failsafe/ui": path.resolve(__dirname, "../failsafe-ui/src/index.ts"),
      "@failsafe/ui/styles.css": path.resolve(__dirname, "../failsafe-ui/src/index.css"),
    },
  },
  clearScreen: false,
  server: {
    port: 5174,
    strictPort: true,
    host: host ?? false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/crates/failsafe-desktop/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "esnext",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    outDir: "dist",
    emptyOutDir: true,
  },
})
