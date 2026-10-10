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

// The backend the dev server proxies to. Default: `AWZY_LISTEN_ADDR=127.0.0.1:8090 bazel run
// //portals/corp/backend:awzy-corp-portal` (the client portal has :8080). Point it at a deployed
// corp portal to run this frontend against it (ADR 0024):
//   AWZY_BACKEND=https://corp.awzysecurity.com pnpm dev
export default defineConfig(({ mode }) => {
  const backend = loadEnv(mode, ".", "AWZY_").AWZY_BACKEND ?? "http://localhost:8090";
  // changeOrigin: the deployed backend sees its own Host header, as it would behind its ingress.
  const proxy = { target: backend, changeOrigin: true, secure: true };
  return {
    plugins: [react(), relay],
    server: {
      port: 5174,
      https,
      strictPort: true,
      proxy: {
        "/graphql": { ...proxy, ws: true },
        "/auth": proxy,
      },
    },
  };
});
