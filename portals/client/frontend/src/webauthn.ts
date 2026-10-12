/**
 * The browser side of WebAuthn (ADR 0059). awzy-challenge sends options in WebAuthn's JSON form
 * (binary fields base64url-encoded), which the browser parses into what `navigator.credentials`
 * takes; its responses go back as base64url for the portal's GraphQL API.
 */

import { isRecord } from "@awzy/ui";

function toBase64url(buffer: ArrayBuffer): string {
  let binary = "";
  for (const byte of new Uint8Array(buffer)) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** Registration options in WebAuthn's JSON form, as awzy-challenge sends them. */
function creationJson(json: string): PublicKeyCredentialCreationOptionsJSON {
  const o: unknown = JSON.parse(json);
  if (!isCreationJson(o)) throw new Error("awzy sent security key options this browser can't read.");
  return o;
}

/** Sign-in options in WebAuthn's JSON form, as awzy-challenge sends them. */
function requestJson(json: string): PublicKeyCredentialRequestOptionsJSON {
  const o: unknown = JSON.parse(json);
  if (!isRequestJson(o)) throw new Error("awzy sent security key options this browser can't read.");
  return o;
}

// The browser checks every field as it parses; these check what it needs to start.
function isCreationJson(o: unknown): o is PublicKeyCredentialCreationOptionsJSON {
  return isRecord(o) && typeof o.challenge === "string" && isRecord(o.rp) && isRecord(o.user);
}

function isRequestJson(o: unknown): o is PublicKeyCredentialRequestOptionsJSON {
  return isRecord(o) && typeof o.challenge === "string";
}

/** Whether this browser can use security keys at all. */
export function webAuthnAvailable(): boolean {
  return typeof window !== "undefined" && "PublicKeyCredential" in window && !!navigator.credentials;
}

/** Registers a new key: runs `navigator.credentials.create()` with awzy's options. */
export async function createCredential(
  optionsJson: string,
): Promise<{ clientDataJson: string; attestationObject: string }> {
  const publicKey = PublicKeyCredential.parseCreationOptionsFromJSON(creationJson(optionsJson));
  const credential = await navigator.credentials.create({ publicKey });
  if (!(credential instanceof PublicKeyCredential)) throw new Error("No security key was registered.");
  const response = credential.response;
  if (!(response instanceof AuthenticatorAttestationResponse)) throw new Error("The key didn't register.");
  return {
    clientDataJson: toBase64url(response.clientDataJSON),
    attestationObject: toBase64url(response.attestationObject),
  };
}

/** Signs awzy's challenge with one of the person's keys: `navigator.credentials.get()`. */
export async function getAssertion(
  ceremonyQid: string,
  optionsJson: string,
): Promise<{
  ceremonyQid: string;
  credentialId: string;
  clientDataJson: string;
  authenticatorData: string;
  signature: string;
}> {
  const publicKey = PublicKeyCredential.parseRequestOptionsFromJSON(requestJson(optionsJson));
  const credential = await navigator.credentials.get({ publicKey });
  if (!(credential instanceof PublicKeyCredential)) throw new Error("No security key answered.");
  const response = credential.response;
  if (!(response instanceof AuthenticatorAssertionResponse)) throw new Error("The key didn't sign.");
  return {
    ceremonyQid,
    credentialId: toBase64url(credential.rawId),
    clientDataJson: toBase64url(response.clientDataJSON),
    authenticatorData: toBase64url(response.authenticatorData),
    signature: toBase64url(response.signature),
  };
}

/** A browser's WebAuthn error, in words a person can act on. */
export function webAuthnMessage(e: unknown): string {
  if (e instanceof DOMException && e.name === "NotAllowedError") {
    return "The security key prompt was cancelled or timed out. Try again and touch your key when it blinks.";
  }
  if (e instanceof DOMException && e.name === "InvalidStateError") {
    return "That security key is already registered.";
  }
  return e instanceof Error ? e.message : String(e);
}
