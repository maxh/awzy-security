// Signs in to the client portal in a real browser, through the dev IdP's redirect flow
// (OIDC authorization code with PKCE), and checks who the portal thinks is signed in.
//
// Starts the dev IdP (HTTPS with a self-signed certificate the portal is told to trust), a PDP
// with the built-in policies (monitor mode, no awzy-policy), and the client portal serving its
// built frontend, each on a free port. Skips if Chrome isn't found.

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { request } from "node:https";
import { tmpdir } from "node:os";
import { createServer } from "node:net";
import { join, resolve } from "node:path";

import puppeteer from "puppeteer-core";

const CHROME = [
  process.env.CHROME_PATH,
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/usr/bin/google-chrome",
  "/usr/bin/google-chrome-stable",
  "/usr/bin/chromium",
].find((p) => p && existsSync(p));

if (!CHROME) {
  console.log("skipping: no Chrome found (set CHROME_PATH)");
  process.exit(0);
}

const freePort = () =>
  new Promise((done) => {
    const s = createServer().listen(0, "127.0.0.1", () => {
      const { port } = s.address();
      s.close(() => done(port));
    });
  });

const children = [];
function start(name, binary, env) {
  const child = spawn(resolve(binary), [], {
    env: { ...process.env, AWZY_LOG_FORMAT: "json", ...env },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let log = "";
  child.stdout.on("data", (d) => (log += d));
  child.stderr.on("data", (d) => (log += d));
  child.on("exit", (code) => code && code !== 143 && console.error(`${name} exited with ${code}:\n${log}`));
  children.push(child);
  return child;
}

// GET over HTTPS trusting `ca`; resolves to the status code.
const httpsStatus = (url, ca) =>
  new Promise((done, fail) =>
    request(url, { ca }, (res) => (res.resume(), done(res.statusCode)))
      .on("error", fail)
      .end(),
  );

async function waitForHttps(url, caFile) {
  for (let i = 0; i < 100; i++) {
    try {
      if (existsSync(caFile) && (await httpsStatus(url, readFileSync(caFile))) === 200) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`${url} never became ready`);
}

async function waitFor(url) {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`${url} never became ready`);
}

function check(ok, message) {
  if (!ok) throw new Error(message);
  console.log(`ok: ${message}`);
}

const [idpPort, pdpPort, portalPort] = await Promise.all([freePort(), freePort(), freePort()]);
const issuer = `https://127.0.0.1:${idpPort}`;
const idpCa = join(mkdtempSync(join(tmpdir(), "awzy-e2e-")), "dev-idp-ca.pem");
const portal = `http://127.0.0.1:${portalPort}`;

let browser;
let exitCode = 1;
try {
  // A self-signed certificate (no .dev-certs here), written out for the portal to trust.
  start("dev-idp", process.env.AWZY_E2E_DEV_IDP, {
    AWZY_LISTEN_ADDR: `127.0.0.1:${idpPort}`,
    AWZY_DEV_IDP_CA_OUT: idpCa,
  });
  start("pdp", process.env.AWZY_E2E_PDP, { AWZY_LISTEN_ADDR: `127.0.0.1:${pdpPort}` });
  // The portal reads the IdP's CA file when it starts, so it waits until the IdP has written
  // it and is serving.
  await waitForHttps(`${issuer}/jwks`, idpCa);
  start("portal", process.env.AWZY_E2E_PORTAL, {
    AWZY_LISTEN_ADDR: `127.0.0.1:${portalPort}`,
    AWZY_PDP_ADDR: `http://127.0.0.1:${pdpPort}`,
    AWZY_AUTH_ISSUER: issuer,
    AWZY_AUTH_JWKS_URL: `${issuer}/jwks`,
    AWZY_AUTH_CA_FILE: idpCa,
    AWZY_PORTAL_STATIC_DIR: resolve(process.env.AWZY_E2E_FRONTEND),
  });
  await Promise.all([waitFor(`http://127.0.0.1:${pdpPort}/readyz`), waitFor(`${portal}/readyz`)]);

  browser = await puppeteer.launch({
    executablePath: CHROME,
    headless: true,
    // The dev IdP's certificate is self-signed in this test; a developer's browser trusts the
    // mkcert CA instead.
    acceptInsecureCerts: true,
    args: process.platform === "linux" ? ["--no-sandbox"] : [],
  });
  const page = await browser.newPage();
  const consoleLog = [];
  page.on("console", (m) => consoleLog.push(`${m.type()}: ${m.text()}`));
  page.on("pageerror", (e) => consoleLog.push(`pageerror: ${e.message}`));
  // On failure: where the browser ended up and what it said.
  globalThis.diagnose = async () => {
    console.error(`page: ${page.url()}`);
    console.error(`text: ${(await page.evaluate(() => document.body.innerText).catch(() => "")).slice(0, 500)}`);
    console.error(`console:\n${consoleLog.join("\n")}`);
    if (process.env.TEST_UNDECLARED_OUTPUTS_DIR) {
      await page.screenshot({ path: `${process.env.TEST_UNDECLARED_OUTPUTS_DIR}/failure.png` }).catch(() => {});
    }
  };

  // Signs in as `name` from the dev IdP's "sign in as" page, then waits for the portal.
  async function signInAs(name) {
    await page.waitForFunction(() => location.pathname === "/authorize", { timeout: 10000 });
    check(page.url().startsWith(issuer), `the portal sent the browser to the IdP (${new URL(page.url()).origin})`);
    const challenge = new URL(page.url()).searchParams;
    check(
      challenge.get("code_challenge_method") === "S256" && challenge.get("code_challenge"),
      "the request uses PKCE (S256)",
    );
    await Promise.all([page.waitForNavigation(), page.locator(`::-p-text(${name})`).click()]);
    await page.waitForFunction((n) => document.body.innerText.includes(n), { timeout: 10000 }, name);
    check(
      new URL(page.url()).origin === portal && !page.url().includes("code="),
      `back in the portal at ${new URL(page.url()).pathname}, signed in as ${name}`,
    );
  }

  // What the backend says about the browser's token: the GraphQL viewer and a PDP check.
  const whoAmI = () =>
    page.evaluate(async () => {
      const key = Object.keys(sessionStorage).find((k) => k.startsWith("oidc.user:"));
      const { access_token } = JSON.parse(sessionStorage.getItem(key));
      const res = await fetch("/graphql", {
        method: "POST",
        headers: { "content-type": "application/json", authorization: `Bearer ${access_token}` },
        body: JSON.stringify({
          query: "{ viewer { id kind } canPublish: viewerCan(action: ADMINISTER, area: POLICIES) }",
        }),
      });
      return (await res.json()).data;
    });

  await page.goto(`${portal}/policies`);
  await signInAs("Alice (security)");
  check(new URL(page.url()).pathname === "/policies", "returned to the page that needed sign-in");
  const alice = await whoAmI();
  check(
    alice.viewer.id === "alice" && alice.viewer.kind === "Human" && alice.canPublish === true,
    "the backend sees alice, who may publish policy",
  );

  // This test runs no awzy-policy, so the Policies page can't load. The error must show inside
  // the page, with the sidebar and its account menu still there.
  await page.waitForFunction(() => document.body.innerText.includes("This page couldn't load"), { timeout: 10000 });
  const text = await page.evaluate(() => document.body.innerText);
  check(
    text.includes("AWZY_POLICY_ADDR") &&
      text.includes("Alice (security)") &&
      (await page.$('[aria-label="Account menu"]')) !== null,
    "a page that fails to load keeps the sidebar and the account menu",
  );

  const anonymous = await page.evaluate(
    async () =>
      (await fetch("/graphql", { method: "POST", headers: { "content-type": "application/json" }, body: "{}" })).status,
  );
  check(anonymous === 401, "a request without the token is refused");

  await page.locator('[aria-label="Account menu"]').click();
  await Promise.all([page.waitForNavigation(), page.locator("::-p-text(Sign out)").click()]);
  await signInAs("Bob (engineering)");
  const bob = await whoAmI();
  check(
    bob.viewer.id === "bob" && bob.canPublish === false,
    "after switching users the backend sees bob, who may not publish",
  );

  exitCode = 0;
} catch (e) {
  console.error(e);
  await globalThis.diagnose?.();
} finally {
  await browser?.close();
  for (const c of children) c.kill("SIGTERM");
}
process.exit(exitCode);
