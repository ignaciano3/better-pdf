import { defineConfig } from "vite";

export default defineConfig({
  // No special WASM plugin needed — we import the .wasm via the `?url` suffix
  // which Vite handles natively, and pass the resulting URL to initializeWasm().
  build: {
    target: "es2022",
  },
});
