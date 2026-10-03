import { DEMO, inDesktopShell } from "../api";

export type DesktopPlatform = "macos" | "windows" | "linux";

export function detectDesktopPlatform(
  userAgent = navigator.userAgent,
  platform = navigator.platform,
): DesktopPlatform | null {
  const haystack = `${platform} ${userAgent}`.toLowerCase();
  if (/mac|iphone|ipad|ipod/.test(haystack)) return "macos";
  if (/win/.test(haystack)) return "windows";
  if (/linux|x11/.test(haystack)) return "linux";
  return null;
}

export function previewChromePlatform(
  search = window.location.search,
  enabled = DEMO,
): DesktopPlatform | null {
  if (!enabled) return null;
  const variant = new URLSearchParams(search).get("ombChrome");
  return variant === "macos" || variant === "windows" || variant === "linux" ? variant : null;
}

export function nativeChromePlatform(): DesktopPlatform | null {
  const preview = previewChromePlatform();
  if (preview) return preview;
  return inDesktopShell() ? detectDesktopPlatform() : null;
}

export function isChromePreview(): boolean {
  return previewChromePlatform() !== null && !inDesktopShell();
}
