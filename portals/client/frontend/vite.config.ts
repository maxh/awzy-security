import { existsSync, readFileSync } from "node:fs";

import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv, type Plugin } from "vite";
import relay from "vite-plugin-relay";

// The queries this build sends by ID, in dist/ beside it: the backend loads them from the frontend
// it serves (ADR 0090). `pnpm relay` or the Bazel `relay` target writes the file first.
const persistedQueries: Plugin = {
  name: "awzy-persisted-queries",
  generateBundle() {
    this.emitFile({
      type: "asset",
      fileName: "persisted_queries.json",
      source: readFileSync("persisted_queries.json", "utf8"),
    });
  },
};

// https://localhost with the mkcert certificate in .dev-certs/ (see the README): the browser
// trusts it, Secure cookies work in every browser, and IdPs that want HTTPS redirects accept it.
// Plain http when it isn't there.
const certs = new URL("../../../.dev-certs/", import.meta.url);
const https = existsSync(new URL("localhost.pem", certs))
  ? { cert: readFileSync(new URL("localhost.pem", certs)), key: readFileSync(new URL("localhost-key.pem", certs)) }
  : undefined;

// The backend the dev server proxies to. Default: `bazel run //portals/client/backend:awzy-client-portal`
// on :8080. Point it at a deployed portal to run this frontend against real data (ADR 0024):
//   AWZY_BACKEND=https://aws.demo.awzysecurity.com VITE_AWZY_CLIENT_ID=awzy-client-portal-dev pnpm dev
export default defineConfig(({ mode }) => {
  const backend = loadEnv(mode, ".", "AWZY_").AWZY_BACKEND ?? "http://localhost:8080";
  // changeOrigin: the deployed backend sees its own Host header, as it would behind its ingress.
  const proxy = { target: backend, changeOrigin: true, secure: true };
  return {
    plugins: [react(), relay, persistedQueries],
    server: {
      port: 5173,
      https,
      strictPort: true,
      proxy: {
        "/graphql": { ...proxy, ws: true },
        "/auth/config": proxy,
      },
    },
  };
});
