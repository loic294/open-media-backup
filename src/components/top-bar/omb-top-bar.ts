import { getCurrentWindow } from "@tauri-apps/api/window";
import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import { DEMO, inDesktopShell } from "../../api";
import { isChromePreview, nativeChromePlatform } from "../../utils/platform";
import { OmbElement } from "../ui/omb-element";
import "./space-pills";
import "./sync-pill";

@customElement("omb-top-bar")
export class OmbTopBar extends OmbElement {
  #minimize() {
    if (inDesktopShell())
      void getCurrentWindow()
        .minimize()
        .catch(() => undefined);
  }

  #toggleMaximize() {
    if (inDesktopShell())
      void getCurrentWindow()
        .toggleMaximize()
        .catch(() => undefined);
  }

  #close() {
    if (inDesktopShell())
      void getCurrentWindow()
        .close()
        .catch(() => undefined);
  }

  override render() {
    const computer = this.store.snapshot?.computer;
    const chrome = nativeChromePlatform();
    const macChrome = chrome === "macos";
    const windowsChrome = chrome === "windows";
    const showPreviewTrafficLights = macChrome && isChromePreview();
    const headerPadding = macChrome ? "pl-24 pr-5" : "px-5";
    return html`
      <header
        class="relative grid grid-cols-[1fr_auto_1fr] items-center gap-4 ${headerPadding} h-16 border-b border-base-300 bg-base-200"
        data-tauri-drag-region="deep"
      >
        ${
          showPreviewTrafficLights
            ? html`<div
                class="absolute left-5 top-1/2 flex -translate-y-1/2 items-center gap-2"
                aria-hidden="true"
              >
                <span class="size-3 rounded-full bg-[#ff5f57]"></span>
                <span class="size-3 rounded-full bg-[#febc2e]"></span>
                <span class="size-3 rounded-full bg-[#28c840]"></span>
              </div>`
            : nothing
        }
        <div class="flex items-center gap-3 min-w-0">
          <span class="grid place-items-center size-9 rounded-box bg-primary text-primary-content shrink-0">
            <omb-icon name="shield-check" class="size-5"></omb-icon>
          </span>
          <div class="min-w-0 leading-tight">
            <div class="font-semibold truncate">Open Media Backup</div>
            <div class="text-xs text-base-content/60 truncate">${computer?.name ?? ""}</div>
          </div>
        </div>
        <omb-space-pills></omb-space-pills>
        <div class="flex items-center justify-end gap-2">
          ${
            DEMO
              ? html`<span
                  class="badge badge-warning badge-soft whitespace-nowrap"
                  title="Demo mode: sample data from a simulated backend. Nothing touches your disks."
                  >Demo data</span
                >`
              : nothing
          }
          <omb-sync-pill></omb-sync-pill>
          <button
            class="btn btn-ghost btn-square btn-sm"
            type="button"
            title="Settings"
            @click=${() => this.store.open({ type: "app-settings" })}
          >
            <omb-icon name="settings" class="size-5"></omb-icon>
          </button>
          ${
            windowsChrome
              ? html`<div
                  class="ml-1 flex items-center border-l border-base-300 pl-1"
                  aria-label="Window controls"
                >
                  <button
                    class="btn btn-ghost btn-square h-10 min-h-0 w-10 rounded-none text-lg font-normal"
                    type="button"
                    title="Minimize"
                    @click=${this.#minimize}
                  >
                    <span aria-hidden="true">−</span><span class="sr-only">Minimize</span>
                  </button>
                  <button
                    class="btn btn-ghost btn-square h-10 min-h-0 w-10 rounded-none text-base font-normal"
                    type="button"
                    title="Maximize"
                    @click=${this.#toggleMaximize}
                  >
                    <span aria-hidden="true">□</span><span class="sr-only">Maximize</span>
                  </button>
                  <button
                    class="btn btn-ghost btn-square h-10 min-h-0 w-10 rounded-none text-base font-normal hover:bg-error hover:text-error-content"
                    type="button"
                    title="Close"
                    @click=${this.#close}
                  >
                    <span aria-hidden="true">×</span><span class="sr-only">Close</span>
                  </button>
                </div>`
              : nothing
          }
        </div>
      </header>
    `;
  }
}
