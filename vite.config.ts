import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// 固定桌面开发端口，避免 Tauri 连接到其他服务；Rust 变更交由 Tauri 监听。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
});
