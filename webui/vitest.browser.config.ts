import vue from "@vitejs/plugin-vue";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  plugins: [vue()],
  optimizeDeps: {
    include: ["vue", "@tabler/icons-vue"],
  },
  test: {
    name: "browser",
    include: ["tests/browser/**/*.test.ts"],
    browser: {
      enabled: true,
      provider: playwright({ launchOptions: { channel: "chrome" } }),
      headless: true,
      instances: [
        {
          browser: "chromium",
        },
      ],
      screenshotFailures: false,
    },
  },
});
