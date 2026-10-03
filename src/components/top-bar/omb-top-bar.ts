import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import { DEMO } from "../../api";
import { OmbElement } from "../ui/omb-element";
import "./space-pills";
import "./sync-pill";

@customElement("omb-top-bar")
export class OmbTopBar extends OmbElement {
  override render() {
    const computer = this.store.snapshot?.computer;
    return html`
      <header class="grid grid-cols-[1fr_auto_1fr] items-center gap-4 px-5 h-16 border-b border-base-300 bg-base-200">
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
          ${DEMO
            ? html`<span class="badge badge-warning badge-soft whitespace-nowrap" title="Demo mode: sample data from a simulated backend. Nothing touches your disks.">Demo data</span>`
            : nothing}
          <omb-sync-pill></omb-sync-pill>
          <button class="btn btn-ghost btn-square btn-sm" title="Settings" @click=${() => this.store.open({ type: "app-settings" })}>
            <omb-icon name="settings" class="size-5"></omb-icon>
          </button>
        </div>
      </header>
    `;
  }
}
