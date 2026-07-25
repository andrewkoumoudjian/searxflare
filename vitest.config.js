import { cloudflareTest } from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./wrangler.toml" },
      miniflare: {
        bindings: {
          API_KEY_SHA256: "4c806362b613f7496abf284146efd31da90e4b16169fe001841ca17290f427c4"
        },
        cf: false
      }
    })
  ],
  test: {
    include: ["tests/integration/**/*.spec.js"],
    testTimeout: 15000
  }
});
