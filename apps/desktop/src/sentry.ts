// Sentry error reporting for the desktop UI (its own Sentry project, separate from the
// Rust core). Enabled only when VITE_SENTRY_DSN is set, so local dev is a no-op.
import * as Sentry from "@sentry/react";

// Baked default for release builds (end-user machines have no env vars); empty in dev
// (`tauri dev` runs Vite in dev mode) so local runs stay quiet. VITE_SENTRY_DSN overrides.
const DSN = "";

export const sentryEnabled = Boolean(DSN);

export function initSentry(): void {
  if (!DSN) return;
  Sentry.init({
    dsn: DSN,
    environment: import.meta.env.MODE,
    tracesSampleRate: 0,
  });
}

export { Sentry };
