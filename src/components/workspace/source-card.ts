import { html, nothing } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import type { Source } from "../../api/types";
import { flowStatus, isRunnable, sourceStatus } from "../../state/derived";
import { deviceById, mappingFor } from "../../state/selectors";
import { fileManagerName } from "../../utils/file-manager";
import { formatBytes, plural } from "../../utils/format";
import { sourceTaskName } from "../../utils/names";
import { DEVICE_ICON, DEVICE_TONE } from "../ui/device-icon";
import { OmbElement } from "../ui/omb-element";
import type { CardContextMenuItem } from "./card-context-menu";
import "./card-context-menu";
import { buildCardContextMenuItems } from "./card-context-menu-model";

/** Safe-copy badge tone: all required copies → success, some → warning, none → error. */
export function safeTone(safe: number, required: number): string {
  if (safe >= required) return "badge-success";
  return safe > 0 ? "badge-warning" : "badge-error";
}

@customElement("omb-source-card")
export class OmbSourceCard extends OmbElement {
  @property({ attribute: false }) source!: Source;
  @state() private menuAt: { x: number; y: number } | null = null;

  #runnableFlows() {
    const { snapshot, status } = this.store;
    return (snapshot?.flows ?? [])
      .filter((flow) => flow.source_id === this.source.id)
      .filter((flow) => {
        const fs = flowStatus(status, flow.id);
        return fs && isRunnable(fs);
      });
  }

  #menuItems(): CardContextMenuItem[] {
    const snapshot = this.store.snapshot;
    const st = sourceStatus(this.store.status, this.source.id);
    const specs = buildCardContextMenuItems({
      fileManagerName: fileManagerName(),
      runnableCount: this.#runnableFlows().length,
      offline: !(st?.available ?? false),
      hasFilesystemPath: !!(snapshot && mappingFor(snapshot, this.source.device_id)),
    });
    return specs.map((item) => ({
      ...item,
      run: () => {
        switch (item.action) {
          case "browse":
            this.store.open({ type: "media-browser", sourceId: this.source.id });
            break;
          case "run":
            void this.#run();
            break;
          case "edit":
            this.store.open({ type: "source-settings", sourceId: this.source.id });
            break;
          case "reveal":
            void this.store.revealInFileManager("source", this.source.id);
            break;
        }
      },
    }));
  }

  async #run() {
    const snapshot = this.store.snapshot;
    for (const flow of this.#runnableFlows()) {
      const dest = snapshot?.destinations.find((d) => d.id === flow.destination_id);
      if ((dest?.kind ?? "folder") === "app") await this.store.openFlowInApp(flow.id);
      else await this.store.runFlow(flow.id);
    }
  }

  #openMenu(event: MouseEvent) {
    event.preventDefault();
    this.menuAt = { x: event.clientX, y: event.clientY };
  }

  #footer() {
    const st = sourceStatus(this.store.status, this.source.id);
    if (!st)
      return this.store.statusLoading
        ? html`<span class="skeleton h-6 w-32"></span>`
        : html`<span class="text-sm text-base-content/60">Status unavailable</span>`;
    if (st.required_copies === null)
      return html`
        <span class="badge badge-ghost gap-1.5">${st.safe_copies} safe copies</span>
        <span class="text-sm text-base-content/60">${st.blocking_reason}</span>
      `;
    const icon =
      st.safe_copies >= st.required_copies ? "shield-check" : st.safe_copies > 0 ? "shield" : "shield-alert";
    return html`
      <span class="badge badge-soft ${safeTone(st.safe_copies, st.required_copies)} gap-1.5">
        <omb-icon name=${icon} class="size-3.5"></omb-icon>${st.safe_copies}/${st.required_copies} safe copies
      </span>
      ${
        st.wipe_eligible && this.store.project
          ? html`<button
              class="btn btn-sm btn-error btn-soft gap-1.5"
              @click=${(e: Event) => (e.stopPropagation(), this.store.open({ type: "wipe-card", sourceId: this.source.id }))}
            >
              <omb-icon name="eraser"></omb-icon>Wipe card
            </button>`
          : html`<span class="text-sm text-base-content/60 truncate">${st.blocking_reason ?? ""}</span>`
      }
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
        @contextmenu=${(event: MouseEvent) => this.#openMenu(event)}
      >
        <div class="card-body p-4 gap-3">
          <div class="flex items-center gap-3">
            <span class="grid place-items-center size-10 rounded-box ${DEVICE_TONE[kind]}"
              ><omb-icon name=${DEVICE_ICON[kind]} class="size-5"></omb-icon
            ></span>
            <div class="flex-1 min-w-0">
              <div class="font-semibold truncate">${sourceTaskName(this.source, device)}</div>
              <div class="text-xs text-base-content/60 truncate">
                Device: ${device?.name ?? "No device selected"}
              </div>
              <div class="text-sm text-base-content/60 truncate">
                ${st?.available ? `${plural(st.file_count, "file")} · ${formatBytes(st.total_bytes)}` : this.source.path_template || "Whole device"}
              </div>
            </div>
            <span class="flex items-center gap-2 text-sm text-base-content/70">
              <span class="status ${st?.available ? "status-success" : "status-neutral"}"></span
              >${!device ? "Select a device" : !st ? (this.store.statusLoading ? "Checking" : "Status unavailable") : st.available ? "Mounted" : "Not mounted"}
            </span>
            <button
              class="btn btn-ghost btn-xs btn-square"
              title="Source settings"
              @click=${(e: Event) => (e.stopPropagation(), this.store.open({ type: "source-settings", sourceId: this.source.id }))}
            >
              <omb-icon name="sliders"></omb-icon>
            </button>
          </div>
          <button
            class="btn btn-sm self-start"
            @click=${(e: Event) => (e.stopPropagation(), this.store.open({ type: "media-browser", sourceId: this.source.id }))}
          >
            <omb-icon name="images"></omb-icon>Browse media
          </button>
          <div class="flex items-center justify-between gap-3 min-h-8">${this.#footer()}</div>
        </div>
        <span
          data-port-source=${this.source.id}
          title="Drag onto a destination to connect"
          class="absolute -right-2 top-1/2 -translate-y-1/2 size-4 rounded-full border-2 border-base-content/60 bg-base-200 cursor-crosshair hover:border-primary hover:scale-125 transition-transform touch-none"
        ></span>
        ${
          this.menuAt
            ? html`<omb-card-context-menu
                .x=${this.menuAt.x}
                .y=${this.menuAt.y}
                .items=${this.#menuItems()}
                @menu-close=${() => (this.menuAt = null)}
              ></omb-card-context-menu>`
            : nothing
        }
      </article>
    `;
  }
}
