import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// In the browser (dev/preview) the daemon is reached through its Unix socket.
// Inside Tauri the Rust shell forwards requests instead, so no TCP port exists.
const socket =
  process.env.ADG_POWERD_SOCKET ??
  `${process.env.XDG_RUNTIME_DIR ?? `/run/user/${process.getuid?.() ?? 1000}`}/adg-power/powerd.sock`;

const proxy = { "/api": { target: { socketPath: socket } as never, changeOrigin: false } };

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: "127.0.0.1", proxy },
  preview: { port: 4173, host: "127.0.0.1", proxy },
  build: { target: "es2022", chunkSizeWarningLimit: 600 },
  test: { environment: "node" },
} as never);
