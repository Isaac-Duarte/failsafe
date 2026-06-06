import path from "path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig, type Plugin } from "vite"

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

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss(), uiPackageAlias()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@failsafe/ui": path.resolve(__dirname, "../failsafe-ui/src/index.ts"),
      "@failsafe/ui/styles.css": path.resolve(__dirname, "../failsafe-ui/src/index.css"),
    },
  },
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8080",
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
})
