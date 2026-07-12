/** true on macOS — the primary shortcut modifier is ⌘ there, Ctrl elsewhere */
export const isMac = /mac/i.test(navigator.platform) || /Macintosh/.test(navigator.userAgent);

/** whether the platform's primary shortcut modifier is held (⌘ on macOS,
 * Ctrl on Windows/Linux). On macOS Ctrl+click is the context-menu gesture,
 * so it must NOT double as a shortcut modifier there. */
export function primaryMod(e: { ctrlKey: boolean; metaKey: boolean }): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}
