import { existsSync, readFileSync } from "node:fs";

import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv } from "vite";
import relay from "vite-plugin-relay";

// https://localhost with the mkcert certificate in .dev-certs/ (see the README): the browser
// trusts it, Secure cookies work in every browser, and IdPs that want HTTPS redirects accept it.
// Plain http when it isn't there.
const certs = new URL("../../../.dev-certs/", import.meta.url);
const https = existsSync(new URL("localhost.pem", certs))
  ? { cert: readFileSync(new URL("localhost.pem", certs)), key: readFileSync(new URL("localhost-key.pem", certs)) }
  : undefined;

// The backend the dev server proxies to. Default: `bazel run //portals/client/backend:awzy-client-portal`
// on :8080. Point it at a deployed portal to run this frontend against real data (ADR 0024):
//   AWZY_BACKEND=https://demo.awzysecurity.com VITE_AWZY_CLIENT_ID=awzy-client-portal-dev pnpm dev
export default defineConfig(({ mode }) => {
  const backend = loadEnv(mode, ".", "AWZY_").AWZY_BACKEND ?? "http://localhost:8080";
  // changeOrigin: the deployed backend sees its own Host header, as it would behind its ingress.
  const proxy = { target: backend, changeOrigin: true, secure: true };
  return {
    plugins: [react(), relay],
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
