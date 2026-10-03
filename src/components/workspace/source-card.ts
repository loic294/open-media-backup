import { html } from "lit";
import { customElement, property } from "lit/decorators.js";
import type { Source } from "../../api/types";
import { sourceStatus } from "../../state/derived";
import { deviceById } from "../../state/selectors";
import { formatBytes, plural } from "../../utils/format";
import { DEVICE_ICON, DEVICE_TONE } from "../ui/device-icon";
import { OmbElement } from "../ui/omb-element";

/** Safe-copy badge tone: all required copies → success, some → warning, none → error. */
export function safeTone(safe: number, required: number): string {
  if (safe >= required) return "badge-success";
  return safe > 0 ? "badge-warning" : "badge-error";
}

@customElement("omb-source-card")
export class OmbSourceCard extends OmbElement {
  @property({ attribute: false }) source!: Source;

  #footer() {
    const st = sourceStatus(this.store.status, this.source.id);
    if (!st) return html`<span class="skeleton h-6 w-32"></span>`;
    const icon = st.safe_copies >= st.required_copies ? "shield-check" : st.safe_copies > 0 ? "shield" : "shield-alert";
    return html`
      <span class="badge badge-soft ${safeTone(st.safe_copies, st.required_copies)} gap-1.5">
        <omb-icon name=${icon} class="size-3.5"></omb-icon>${st.safe_copies}/${st.required_copies} safe copies
      </span>
      ${st.wipe_eligible
        ? html`<button
            class="btn btn-sm btn-error btn-soft gap-1.5"
            @click=${(e: Event) => (e.stopPropagation(), this.store.open({ type: "wipe-card", sourceId: this.source.id }))}
          >
            <omb-icon name="eraser"></omb-icon>Wipe card
          </button>`
        : html`<span class="text-sm text-base-content/60 truncate">${st.blocking_reason ?? ""}</span>`}
    `;
  }

  override render() {
    const { snapshot, status, selectedSourceId } = this.store;
    const device = snapshot ? deviceById(snapshot, this.source.device_id) : undefined;
    const st = sourceStatus(status, this.source.id);
    const kind = device?.kind ?? "other";
    const selected = selectedSourceId === this.source.id;
    return html`
      <article
        data-source-id=${this.source.id}
        class="relative card bg-base-100 border cursor-pointer transition-colors ${selected ? "border-primary ring-1 ring-primary" : "border-base-300 hover:border-base-content/20"}"
        @click=${(e: Event) => !(e.target as Element).closest("[data-port-source]") && this.store.select(this.source.id)}
      >
        <div class="card-body p-4 gap-3">
          <div class="flex items-center gap-3">
            <span class="grid place-items-center size-10 rounded-box ${DEVICE_TONE[kind]}"><omb-icon name=${DEVICE_ICON[kind]} class="size-5"></omb-icon></span>
            <div class="flex-1 min-w-0">
              <div class="font-semibold truncate">${device?.name ?? "Unknown device"}</div>
              <div class="text-sm text-base-content/60 truncate">
                ${st?.available ? `${plural(st.file_count, "file")} · ${formatBytes(st.total_bytes)}` : (this.source.path_template || "Whole device")}
              </div>
            </div>
            <span class="flex items-center gap-2 text-sm text-base-content/70">
              <span class="status ${st?.available ? "status-success" : "status-neutral"}"></span>${st?.available ? "Mounted" : "Not mounted"}
            </span>
            <button
              class="btn btn-ghost btn-xs btn-square"
              title="Source settings"
              @click=${(e: Event) => (e.stopPropagation(), this.store.open({ type: "source-settings", sourceId: this.source.id }))}
            >
              <omb-icon name="sliders"></omb-icon>
            </button>
          </div>
          <div class="flex items-center justify-between gap-3 min-h-8">${this.#footer()}</div>
        </div>
        <span
          data-port-source=${this.source.id}
          title="Drag onto a destination to connect"
          class="absolute -right-2 top-1/2 -translate-y-1/2 size-4 rounded-full border-2 border-base-content/60 bg-base-200 cursor-crosshair hover:border-primary hover:scale-125 transition-transform touch-none"
        ></span>
      </article>
    `;
  }
}
