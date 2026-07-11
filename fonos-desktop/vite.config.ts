import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    warmup: {
      clientFiles: ["./src/main.tsx", "./src/App.tsx"],
    },
  },
  optimizeDeps: {
    include: ["react", "react-dom", "@tauri-apps/api"],
  },
  build: {
    target: "safari15",
  },
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
