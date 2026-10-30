export const isMac =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);

/** App-Kürzel: ⌘<key> auf macOS, Strg+Shift+<key> sonst — Strg+<key> allein gehört der Shell. */
export function isAppShortcut(e: KeyboardEvent, key: string): boolean {
  if (e.altKey || e.key.toLowerCase() !== key) return false;
  return isMac
    ? e.metaKey && !e.ctrlKey && !e.shiftKey
    : e.ctrlKey && e.shiftKey && !e.metaKey;
}

export function shortcutLabel(key: string): string {
  return isMac ? `⌘${key.toUpperCase()}` : `Strg+Shift+${key.toUpperCase()}`;
}
