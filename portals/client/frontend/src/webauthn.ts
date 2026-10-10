/**
 * The browser side of WebAuthn (ADR 0059). awzy-challenge sends options as JSON with binary
 * fields base64url-encoded; these turn them into what `navigator.credentials` takes, and its
 * responses back into base64url for the portal's GraphQL API.
 */

function toBase64url(buffer: ArrayBuffer): string {
  let binary = "";
  for (const byte of new Uint8Array(buffer)) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromBase64url(text: string): ArrayBuffer {
  const base64 = text.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(base64 + "=".repeat((4 - (base64.length % 4)) % 4));
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes.buffer;
}

type Descriptor = { type: "public-key"; id: string };

function descriptors(list: ReadonlyArray<Descriptor> | undefined): PublicKeyCredentialDescriptor[] {
  return (list ?? []).map((d) => ({ type: d.type, id: fromBase64url(d.id) }));
}

/** Whether this browser can use security keys at all. */
export function webAuthnAvailable(): boolean {
  return typeof window !== "undefined" && "PublicKeyCredential" in window && !!navigator.credentials;
}

/** Registers a new key: runs `navigator.credentials.create()` with awzy's options. */
export async function createCredential(
  optionsJson: string,
): Promise<{ clientDataJson: string; attestationObject: string }> {
  const o = JSON.parse(optionsJson);
  const credential = (await navigator.credentials.create({
    publicKey: {
      ...o,
      challenge: fromBase64url(o.challenge),
      user: { ...o.user, id: fromBase64url(o.user.id) },
      excludeCredentials: descriptors(o.excludeCredentials),
    },
  })) as PublicKeyCredential | null;
  if (!credential) throw new Error("No security key was registered.");
  const response = credential.response as AuthenticatorAttestationResponse;
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
  const o = JSON.parse(optionsJson);
  const credential = (await navigator.credentials.get({
    publicKey: { ...o, challenge: fromBase64url(o.challenge), allowCredentials: descriptors(o.allowCredentials) },
  })) as PublicKeyCredential | null;
  if (!credential) throw new Error("No security key answered.");
  const response = credential.response as AuthenticatorAssertionResponse;
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
