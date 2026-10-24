import type { MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Manual titlebar drag. Tauri's `data-tauri-drag-region` shim only starts a
 * drag when the OS click-count is exactly 1, so quick successive grabs (which
 * macOS counts as clicks 2, 3, …) are silently ignored and the bar feels dead.
 * Calling startDragging ourselves on every left-button press fixes that.
 */
export function dragWindow(e: MouseEvent) {
  if (e.button !== 0) return;
  // keep the tray widget (trigger button + popover) fully interactive
  if ((e.target as HTMLElement).closest(".tray")) return;
  e.preventDefault();
  getCurrentWindow().startDragging();
}
