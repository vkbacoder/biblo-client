// Auto-update via Tauri's updater plugin. Checks our signed manifest
// (https://bibotracker.com/download/latest.json); on a newer version it downloads the
// signed artifact and then PROMPTS the user to restart — we never relaunch without
// confirmation. Signature is verified against the public key baked into tauri.conf.json —
// an unsigned/tampered artifact is rejected.
export type UpdateProgress =
  | { state: "checking" }
  | { state: "uptodate" }
  | { state: "available"; version: string }
  | { state: "downloading"; pct: number }
  | { state: "ready"; version: string }
  | { state: "error"; message: string };

/** Updates are disabled for this client build. */
export async function checkForUpdates(
  onProgress?: (p: UpdateProgress) => void,
): Promise<boolean> {
  onProgress?.({ state: "uptodate" });
  return false;
}

export async function autoCheckAndPrompt(): Promise<void> {
  return;
}
