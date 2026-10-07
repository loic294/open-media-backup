import { html, nothing } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import type { Source } from "../../api/types";
import { flowStatus, isRunnable, sourceStatus } from "../../state/derived";
import { deviceById, deviceHosts, mappingFor } from "../../state/selectors";
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

  #canOfferWipe(): boolean {
    return this.source.offer_wipe && !!sourceStatus(this.store.status, this.source.id)?.available;
  }

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
      manualWipeDisabled:
        !snapshot ||
        !deviceById(snapshot, this.source.device_id) ||
        deviceById(snapshot, this.source.device_id)?.role === "final",
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
          case "manual-wipe":
            this.store.confirmSourceManuallyWiped(this.source.id);
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
    return this.#canOfferWipe() ? this.#wipeButton() : nothing;
  }

  #safeCopiesIndicator() {
    const st = sourceStatus(this.store.status, this.source.id);
    if (!st) return nothing;
    if (st.required_copies === null)
      return html`
        <button
          type="button"
          class="badge badge-ghost gap-1.5 cursor-pointer hover:brightness-95 focus-visible:outline-2 focus-visible:outline-primary"
          aria-label="View safe-copy details"
          @click=${(e: Event) => (e.stopPropagation(), this.store.open({ type: "safe-copy", sourceId: this.source.id }))}
        >
          ${st.safe_copies} safe copies
        </button>
      `;
    const icon =
      st.safe_copies >= st.required_copies ? "shield-check" : st.safe_copies > 0 ? "shield" : "shield-alert";
    return html`
      <button
        type="button"
        class="badge badge-soft ${safeTone(st.safe_copies, st.required_copies)} gap-1.5 cursor-pointer hover:brightness-95 focus-visible:outline-2 focus-visible:outline-primary"
        aria-label="View safe-copy details"
        @click=${(e: Event) => (e.stopPropagation(), this.store.open({ type: "safe-copy", sourceId: this.source.id }))}
      >
        <omb-icon name=${icon} class="size-3.5"></omb-icon>${st.safe_copies}/${st.required_copies} safe copies
      </button>
    `;
  }

  #mountIndicator() {
    const { snapshot, status } = this.store;
    const st = sourceStatus(status, this.source.id);
    const device = snapshot ? deviceById(snapshot, this.source.device_id) : undefined;
    if (!device || !st)
      return html`<span class="text-sm text-base-content/60"
        >${!device ? "Select a device" : this.store.statusLoading ? "Checking" : "Status unavailable"}</span
      >`;

    const volume = this.store.volumes.find((item) => item.device_id === device.id);
    const mappedRoot = snapshot ? mappingFor(snapshot, device.id)?.root_path : undefined;
    const hostNames = snapshot ? deviceHosts(snapshot, device.id) : [];
    const details = st.available
      ? [
          volume?.name ? `Volume: ${volume.name}` : null,
          volume?.mount_path || st.root_path
            ? `Mounted at ${volume?.mount_path ?? st.root_path}`
            : "Device is available in this workspace.",
          volume?.free_bytes != null && volume.total_bytes != null
            ? `${formatBytes(volume.free_bytes)} free of ${formatBytes(volume.total_bytes)}`
            : null,
        ].filter((line): line is string => !!line)
      : [
          `${device.name} is not currently available on this computer.`,
          mappedRoot ? `Configured location: ${mappedRoot}` : null,
          hostNames.length ? `Known on: ${hostNames.join(", ")}` : null,
        ].filter((line): line is string => !!line);
    const tooltipId = `source-mount-tooltip-${this.source.id}`;
    const label = st.available ? "Mounted" : "Not mounted";
    return html`
      <span class="tooltip tooltip-top tooltip-center">
        <span
          id=${tooltipId}
          role="tooltip"
          class="tooltip-content pointer-events-none z-50 max-w-[calc(100vw-2rem)] rounded-box bg-neutral p-3 text-left text-neutral-content shadow-lg"
        >
          <span class="block font-semibold">${device.name} · ${label}</span>
          ${details.map((line) => html`<span class="block break-words">${line}</span>`)}
          ${this.source.offer_wipe && st.blocking_reason ? html`<span class="block break-words">${st.blocking_reason}</span>` : nothing}
        </span>
        <span
          class="badge badge-soft ${st.available ? "badge-success" : "badge-ghost"} cursor-help"
          tabindex="0"
          role="status"
          aria-label=${label}
          aria-describedby=${tooltipId}
        >
          <span class="status ${st.available ? "status-success" : "status-neutral"}"></span>${label}
        </span>
      </span>
    `;
  }

  #wipeButton() {
    return html`<button
      class="btn btn-sm btn-error btn-soft gap-1.5"
      @click=${(e: Event) => (e.stopPropagation(), this.store.open({ type: "wipe-card", sourceId: this.source.id }))}
    >
      <omb-icon name="eraser"></omb-icon>Wipe card
    </button>`;
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
            <button
              class="btn btn-ghost btn-xs btn-square"
              title="Source settings"
              @click=${(e: Event) => (e.stopPropagation(), this.store.open({ type: "source-settings", sourceId: this.source.id }))}
            >
              <omb-icon name="sliders"></omb-icon>
            </button>
          </div>
          <div class="flex flex-wrap items-center justify-between gap-2">
            <button
              class="btn btn-sm"
              @click=${(e: Event) => (e.stopPropagation(), this.store.open({ type: "media-browser", sourceId: this.source.id }))}
            >
              <omb-icon name="images"></omb-icon>Browse media
            </button>
            <div class="flex flex-wrap items-center justify-end gap-1">
              ${this.#safeCopiesIndicator()} ${this.#mountIndicator()}
            </div>
          </div>
          ${
            this.#canOfferWipe()
              ? html`<div data-source-wipe-footer class="flex items-center justify-between gap-3 min-h-8">
                  ${this.#footer()}
                </div>`
              : nothing
          }
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
