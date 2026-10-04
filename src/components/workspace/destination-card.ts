import { html, nothing } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import type { Destination, DestinationStatus, FileCategory } from "../../api/types";
import { destinationStatus, flowStatus, isRunnable } from "../../state/derived";
import { deviceById, deviceHosts, mappingFor } from "../../state/selectors";
import { estimateTransferSeconds } from "../../utils/eta";
import { fileManagerName } from "../../utils/file-manager";
import { formatBytes, formatCount, formatEta } from "../../utils/format";
import { destinationTaskName } from "../../utils/names";
import { appDisplayName, configuredDestinationApp } from "../../utils/preview-apps";
import { DEVICE_ICON, DEVICE_TONE } from "../ui/device-icon";
import { OmbElement } from "../ui/omb-element";
import type { CardContextMenuItem } from "./card-context-menu";
import "./card-context-menu";
import { buildCardContextMenuItems } from "./card-context-menu-model";

@customElement("omb-destination-card")
export class OmbDestinationCard extends OmbElement {
  @property({ attribute: false }) destination!: Destination;
  @state() private menuAt: { x: number; y: number } | null = null;

  get #incoming() {
    return (this.store.snapshot?.flows ?? []).filter((f) => f.destination_id === this.destination.id);
  }

  #preview(category?: FileCategory) {
    this.store.open({ type: "preview", flowId: this.#incoming[0]?.id ?? null, category });
  }

  #runnableIncoming() {
    return this.#incoming.filter((flow) => {
      const fs = flowStatus(this.store.status, flow.id);
      return fs && isRunnable(fs);
    });
  }

  async #run() {
    for (const f of this.#runnableIncoming()) await this.store.runFlow(f.id);
  }

  async #openInApp() {
    for (const f of this.#runnableIncoming()) await this.store.openFlowInApp(f.id);
  }

  #openMenu(event: MouseEvent) {
    event.preventDefault();
    this.menuAt = { x: event.clientX, y: event.clientY };
  }

  #contextMenu(isApp: boolean, online: boolean): unknown {
    if (!this.menuAt) return nothing;
    const snapshot = this.store.snapshot;
    const specs = buildCardContextMenuItems({
      fileManagerName: fileManagerName(),
      runnableCount: this.#runnableIncoming().length,
      offline: !online,
      hasFilesystemPath: !!(snapshot && mappingFor(snapshot, this.destination.device_id)),
      isAppDestination: isApp,
      browseDisabled: !this.#incoming.length,
    });
    const items: CardContextMenuItem[] = specs.map((item) => ({
      ...item,
      run: () => {
        switch (item.action) {
          case "browse":
            this.#preview();
            break;
          case "run":
            void (isApp ? this.#openInApp() : this.#run());
            break;
          case "edit":
            this.store.open({ type: "destination-settings", destinationId: this.destination.id });
            break;
          case "reveal":
            void this.store.revealInFileManager("destination", this.destination.id);
            break;
        }
      },
    }));
    return html`<omb-card-context-menu
      .x=${this.menuAt.x}
      .y=${this.menuAt.y}
      .items=${items}
      @menu-close=${() => (this.menuAt = null)}
    ></omb-card-context-menu>`;
  }

  #statusLine(st: DestinationStatus, hosts: string[]) {
    const size = formatBytes(st.bytes_to_transfer);
    if (st.last_error && !this.#runnableIncoming().length) {
      return html`<omb-icon name="circle-alert" class="size-5 text-error"></omb-icon>
        <span class="text-sm text-error">${st.last_error}</span>`;
    }
    if (st.failed) {
      return html`<omb-icon name="circle-x" class="size-5 text-error"></omb-icon>
        <button class="font-semibold text-error hover:underline" @click=${() => this.#preview("error")}>
          ${formatCount(st.failed)} failed
        </button>
        <span class="text-sm text-base-content/60 truncate">${st.last_error ?? ""}</span>`;
    }
    if (st.to_transfer && !st.available) {
      const when = hosts.length ? `runs when ${hosts[0]} is online` : "connect the device to run";
      return html`<omb-icon name="clock" class="size-5 text-base-content/50"></omb-icon>
        <button class="font-semibold hover:underline" @click=${() => this.#preview("to_transfer")}>
          ${formatCount(st.to_transfer)} waiting
        </button>
        <span class="text-sm text-base-content/50 truncate">${size} · ${when}</span>`;
    }
    if (st.to_transfer) {
      const eta = formatEta(
        estimateTransferSeconds(
          st.bytes_to_transfer,
          this.store.snapshot?.settings.transfer_speeds,
          this.destination.device_id,
        ),
      );
      return html`<omb-icon name="clock" class="size-5 text-warning"></omb-icon>
        <button
          class="font-semibold text-warning hover:underline"
          @click=${() => this.#preview("to_transfer")}
        >
          ${formatCount(st.to_transfer)} to transfer
        </button>
        <span class="text-sm text-base-content/60 truncate">${size}${eta ? ` · ${eta}` : ""}</span>`;
    }
    if (st.transferred) {
      return html`<omb-icon name="circle-check" class="size-5 text-success"></omb-icon
        ><span class="font-semibold text-success">All transferred</span>`;
    }
    return html`<span class="text-sm text-base-content/50"
      >${this.#incoming.length ? "Nothing to transfer" : "Connect a source to start"}</span
    >`;
  }

  override render() {
    const { snapshot, status, space } = this.store;
    if (!snapshot) return nothing;
    const isApp = (this.destination.kind ?? "folder") === "app";
    const device = deviceById(snapshot, this.destination.device_id);
    const st = destinationStatus(status, this.destination.id);
    const kind = isApp ? "computer" : (device?.kind ?? "other");
    const hosts = deviceHosts(snapshot, this.destination.device_id);
    const online = isApp || (st?.available ?? false);
    const localAppPath = configuredDestinationApp(snapshot.settings, this.destination.id);
    const appName =
      this.destination.app_name || (localAppPath ? appDisplayName(localAppPath) : "Application");
    const details = [
      isApp ? "App · manual import" : device?.description,
      online && st?.free_bytes != null ? `${formatBytes(st.free_bytes)} free` : null,
      !online && hosts.length ? `on ${hosts.join(", ")}` : null,
    ].filter(Boolean);
    const temporaryCopiesPerFinal = space?.temporary_copies_per_final ?? 0;
    const verified =
      this.destination.counts_as_safe_copy &&
      (isApp || device?.role === "final" || (device?.role === "temporary" && temporaryCopiesPerFinal > 0));
    const safeCopyTitle = isApp
      ? "Confirmed imports count as a safe copy"
      : device?.role === "temporary"
        ? `Counts toward safe copies in groups of ${temporaryCopiesPerFinal}`
        : "Counts as a safe copy";
    const canRun = online && this.#runnableIncoming().length > 0;
    const missingStatus = this.store.statusLoading
      ? html`<span class="skeleton h-5 w-48"></span>`
      : html`<span class="text-sm text-base-content/60">Status unavailable</span>`;
    if (isApp) {
      return html`
        <article
          data-dest-id=${this.destination.id}
          class="card bg-base-100 border border-base-300 transition-colors"
          @contextmenu=${(event: MouseEvent) => this.#openMenu(event)}
        >
          <div class="card-body p-4 gap-3">
            <div class="flex items-center gap-3">
              <span class="grid place-items-center size-10 rounded-box bg-info/15 text-info">
                <omb-icon name="external-link" class="size-5"></omb-icon>
              </span>
              <div class="flex-1 min-w-0">
                <div class="font-semibold truncate flex items-center gap-1.5">
                  ${destinationTaskName(this.destination, device, localAppPath)}
                  <span class="badge badge-ghost badge-xs">Manual</span>
                  ${
                    verified
                      ? html`<omb-icon
                          name="shield-check"
                          class="size-4 text-success"
                          title=${safeCopyTitle}
                        ></omb-icon>`
                      : nothing
                  }
                </div>
                <div class="text-sm text-base-content/60 truncate">${appName} · App · manual import</div>
              </div>
              <button
                class="btn btn-ghost btn-xs btn-square"
                title="Destination settings"
                @click=${() => this.store.open({ type: "destination-settings", destinationId: this.destination.id })}
              >
                <omb-icon name="sliders"></omb-icon>
              </button>
            </div>
            <div class="flex items-center gap-2 min-h-8">
              ${
                !localAppPath
                  ? html`<span class="text-sm text-warning"
                      >Choose the app for this destination on this computer.</span
                    >`
                  : nothing
              }
              ${
                localAppPath && st
                  ? st.to_transfer
                    ? html`<omb-icon name="clock" class="size-5 text-info"></omb-icon>
                        <button
                          class="font-semibold text-info hover:underline"
                          @click=${() => this.#preview("to_transfer")}
                        >
                          ${formatCount(st.to_transfer)} to import
                        </button>
                        <span class="text-sm text-base-content/60 truncate"
                          >${formatBytes(st.bytes_to_transfer)} · manual only</span
                        >`
                    : st.transferred
                      ? html`<omb-icon name="circle-check" class="size-5 text-success"></omb-icon>
                          <span class="font-semibold text-success">All imported</span>`
                      : html`<span class="text-sm text-base-content/50">Nothing to import</span>`
                  : localAppPath
                    ? missingStatus
                    : nothing
              }
              <span class="flex-1"></span>
              ${
                st && (st.transferred || st.ignored)
                  ? html`<span class="text-sm text-base-content/60 whitespace-nowrap">
                      <button class="hover:underline" @click=${() => this.#preview("transferred")}>
                        ${formatCount(st.transferred)} done
                      </button>
                      ·
                      <button class="hover:underline" @click=${() => this.#preview("ignored")}>
                        ${formatCount(st.ignored)} ignored
                      </button>
                    </span>`
                  : nothing
              }
              <button
                class="btn btn-ghost btn-sm btn-square"
                title="Preview files"
                ?disabled=${!this.#incoming.length}
                @click=${() => this.#preview()}
              >
                <omb-icon name="images"></omb-icon>
              </button>
              <button
                class="btn ${localAppPath ? "btn-primary" : ""} btn-sm gap-1.5"
                ?disabled=${localAppPath ? !canRun : false}
                @click=${() =>
                  localAppPath
                    ? this.#openInApp()
                    : this.store.open({ type: "destination-settings", destinationId: this.destination.id })}
              >
                <omb-icon name="external-link"></omb-icon
                >${localAppPath ? `Open in ${appName}` : "Choose app…"}
              </button>
            </div>
          </div>
          ${this.#contextMenu(isApp, !!localAppPath)}
        </article>
      `;
    }
    return html`
      <article
        data-dest-id=${this.destination.id}
        class="card bg-base-100 border border-base-300 transition-colors"
        @contextmenu=${(event: MouseEvent) => this.#openMenu(event)}
      >
        <div class="card-body p-4 gap-3 ${online ? "" : "opacity-70"}">
          <div class="flex items-center gap-3">
            <span
              class="grid place-items-center size-10 rounded-box ${online ? DEVICE_TONE[kind] : "bg-base-300 text-base-content/50"}"
            >
              <omb-icon name=${DEVICE_ICON[kind]} class="size-5"></omb-icon>
            </span>
            <div class="flex-1 min-w-0">
              <div class="font-semibold truncate flex items-center gap-1.5">
                ${destinationTaskName(this.destination, device)}
                ${
                  verified
                    ? html`<omb-icon
                        name="shield-check"
                        class="size-4 text-success"
                        title=${safeCopyTitle}
                      ></omb-icon>`
                    : nothing
                }
                ${device?.role === "temporary" ? html`<span class="badge badge-ghost badge-xs">temporary</span>` : nothing}
              </div>
              <div class="text-xs text-base-content/60 truncate">
                Device: ${device?.name ?? "No device selected"}
              </div>
              <div class="text-sm text-base-content/60 truncate">
                ${details.join(" · ") || this.destination.path_template}
              </div>
            </div>
            <span class="flex items-center gap-2 text-sm text-base-content/70">
              <span class="status ${online ? "status-success" : "status-neutral"}"></span
              >${!device ? "Select a device" : !st ? (this.store.statusLoading ? "Checking" : "Status unavailable") : online ? "Online" : "Offline"}
            </span>
            <button
              class="btn btn-ghost btn-xs btn-square"
              title="Destination settings"
              @click=${() => this.store.open({ type: "destination-settings", destinationId: this.destination.id })}
            >
              <omb-icon name="sliders"></omb-icon>
            </button>
          </div>
          <div class="flex items-center gap-2 min-h-8">
            ${st ? this.#statusLine(st, hosts) : missingStatus}
            <span class="flex-1"></span>
            ${
              st && (st.transferred || st.ignored)
                ? html`<span class="text-sm text-base-content/60 whitespace-nowrap">
                    <button class="hover:underline" @click=${() => this.#preview("transferred")}>
                      ${formatCount(st.transferred)} done
                    </button>
                    ·
                    <button class="hover:underline" @click=${() => this.#preview("ignored")}>
                      ${formatCount(st.ignored)} ignored
                    </button>
                  </span>`
                : nothing
            }
            <button
              class="btn btn-ghost btn-sm btn-square"
              title="Preview files"
              ?disabled=${!this.#incoming.length}
              @click=${() => this.#preview()}
            >
              <omb-icon name="images"></omb-icon>
            </button>
            ${
              st?.failed
                ? html`<button
                    class="btn btn-sm btn-error btn-soft gap-1.5"
                    ?disabled=${!canRun}
                    @click=${() => this.#run()}
                  >
                    <omb-icon name="rotate-ccw"></omb-icon>Retry
                  </button>`
                : html`<button class="btn btn-sm gap-1.5" ?disabled=${!canRun} @click=${() => this.#run()}>
                    <omb-icon name="play"></omb-icon>Run
                  </button>`
            }
          </div>
        </div>
        ${this.#contextMenu(isApp, online)}
      </article>
    `;
  }
}
