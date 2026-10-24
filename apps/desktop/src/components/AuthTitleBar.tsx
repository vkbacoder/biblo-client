import { dragWindow } from "./dragWindow";

/**
 * AuthTitleBar — the custom (Overlay) window title bar for the auth / onboarding
 * surfaces: a draggable strip with the app name centered. The native macOS
 * traffic lights overlay the left. No tray/tracking icon here — tracking is
 * paused until setup completes, so there's no state to show. "BiBoTracking"
 * is the brand and stays verbatim in every locale.
 */
export function AuthTitleBar() {
  return (
    <div className="welcome-titlebar" onMouseDown={dragWindow}>
      <span className="welcome-titlebar-title">BiBoTracking</span>
    </div>
  );
}
