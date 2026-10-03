/**
 * `<details class="dropdown">` menus position without CSS anchor positioning (unsupported in older
 * WKWebView), but only toggle from their summary. This closes them on outside click and Escape.
 */
const OPEN = "details.dropdown[open]";

export function closeDropdown(from: Element) {
  from.closest("details")?.removeAttribute("open");
}

function closeAll(except?: Node | null) {
  for (const d of document.querySelectorAll<HTMLDetailsElement>(OPEN)) {
    if (!except || !d.contains(except)) d.open = false;
  }
}

let installed = false;

export function installDropdownDismiss() {
  if (installed) return;
  installed = true;
  document.addEventListener("pointerdown", (e) => closeAll(e.target as Node));
  document.addEventListener("keydown", (e) => e.key === "Escape" && closeAll());
}
