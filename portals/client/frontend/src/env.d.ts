// The Vite variables this frontend reads (see vite.config.ts), so they're typed rather than any.
interface ImportMetaEnv {
  /** An IdP client for running this frontend locally against a deployed backend. */
  readonly VITE_AWZY_CLIENT_ID?: string;
}
