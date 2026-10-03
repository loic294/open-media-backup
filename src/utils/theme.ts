import type { ThemePreference } from "../api/types";

const query = () => window.matchMedia?.("(prefers-color-scheme: dark)");

export function resolveTheme(pref: ThemePreference, prefersDark = query()?.matches ?? false): "omb-light" | "omb-dark" {
  if (pref === "light") return "omb-light";
  if (pref === "dark") return "omb-dark";
  return prefersDark ? "omb-dark" : "omb-light";
}

let current: ThemePreference = "system";
let listening = false;

/** Applies the theme to <html> and follows OS changes while the preference is "system". */
export function applyTheme(pref: ThemePreference): void {
  current = pref;
  document.documentElement.dataset.theme = resolveTheme(pref);
  if (!listening) {
    listening = true;
    query()?.addEventListener("change", () => applyTheme(current));
  }
}
