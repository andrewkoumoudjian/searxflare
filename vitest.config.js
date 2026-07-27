import { cloudflareTest } from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./wrangler.test.toml" },
      miniflare: {
        bindings: {
          API_KEY_SHA256: "4c806362b613f7496abf284146efd31da90e4b16169fe001841ca17290f427c4",
          ENABLE_AI_SEARCH: "false",
          ENABLE_GOOGLE: "true",
          CURSOR_SIGNING_KEY: "integration-test-cursor-signing-key",
          WEB_BOT_AUTH_PRIVATE_KEY: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
          WEB_BOT_AUTH_KEY_ID: "https://example.com/.well-known/http-message-signatures-directory#test",
          WEB_BOT_AUTH_DIRECTORY_URL: "https://example.com/.well-known/http-message-signatures-directory",
          WEB_BOT_AUTH_PUBLIC_JWKS: "{\"keys\":[{\"kid\":\"test\",\"kty\":\"OKP\",\"crv\":\"Ed25519\",\"x\":\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"}]}"
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
