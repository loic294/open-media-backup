import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { mountedFirst, mountedFirstInSlots, spaceDestinations, spaceSources } from "../../state/selectors";
import { closeDropdown } from "../ui/dropdown";
import { OmbElement } from "../ui/omb-element";
import { filterDestinations, filterSources, type DeviceFilter } from "./device-filter";
import "./destination-card";
import "./flow-canvas";
import "./new-volumes";
import "./source-card";

/** Sources (left), connections (middle) and destinations (right). */
@customElement("omb-flow-board")
export class OmbFlowBoard extends OmbElement {
  @state() private sourceFilter: DeviceFilter = { kind: "all" };
  @state() private destinationFilter: DeviceFilter = { kind: "all" };

  #filterDevices(side: "source" | "destination") {
    const { snapshot, space } = this.store;
    if (!snapshot || !space) return [];
    const deviceIds =
      side === "source"
        ? spaceSources(snapshot, space.id).map((source) => source.device_id)
        : spaceDestinations(snapshot, space.id)
            .filter((destination) => (destination.kind ?? "folder") !== "app")
            .map((destination) => destination.device_id);
    const available = new Set(deviceIds.filter(Boolean));
    return snapshot.devices
      .filter((device) => available.has(device.id))
      .sort((a, b) => a.name.localeCompare(b.name));
  }

  override willUpdate(): void {
    const missing = (side: "source" | "destination", filter: DeviceFilter) =>
      filter.kind === "device" &&
      !this.#filterDevices(side).some((device) => device.id === filter.deviceId);
    if (missing("source", this.sourceFilter)) this.sourceFilter = { kind: "all" };
    if (missing("destination", this.destinationFilter)) this.destinationFilter = { kind: "all" };
  }

  override updated(): void {
    // Filtering can move ports without resizing the board.
    this.querySelector<OmbElement>("omb-flow-canvas")?.requestUpdate();
  }

  #selectFilter(event: Event, side: "source" | "destination", filter: DeviceFilter) {
    event.stopPropagation();
    const details = (event.currentTarget as Element).closest("details");
    closeDropdown(event.currentTarget as Element);
    details?.querySelector<HTMLElement>("summary")?.focus();
    if (side === "source") this.sourceFilter = filter;
    else this.destinationFilter = filter;
  }

  #filterMenu(side: "source" | "destination") {
    const noun = side === "source" ? "sources" : "destinations";
    const devices = this.#filterDevices(side);
    const current = side === "source" ? this.sourceFilter : this.destinationFilter;
    const activeLabel =
      current.kind === "device"
        ? (devices.find((device) => device.id === current.deviceId)?.name ?? "All devices")
        : current.kind === "mounted"
          ? "Mounted devices"
          : "All devices";
    const option = (text: string, filter: DeviceFilter, selected: boolean) => html`
      <li>
        <button
          type="button"
          class=${selected ? "menu-active" : ""}
          aria-pressed=${selected}
          @click=${(event: Event) => this.#selectFilter(event, side, filter)}
        >
          <span class="flex-1 truncate">${text}</span>
          ${selected ? html`<omb-icon name="check" aria-hidden="true"></omb-icon>` : nothing}
        </button>
      </li>
    `;
    return html`
      <details
        class="dropdown dropdown-end"
        data-device-filter=${side}
        @click=${(event: Event) => event.stopPropagation()}
        @keydown=${(event: KeyboardEvent) => {
          if (event.key !== "Escape") return;
          const details = event.currentTarget as HTMLDetailsElement;
          details.open = false;
          details.querySelector<HTMLElement>("summary")?.focus();
          event.stopPropagation();
        }}
      >
        <summary
          class="btn btn-sm btn-outline border-base-300 gap-1.5 font-normal ${current.kind === "all" ? "btn-square" : ""}"
          aria-label=${`Filter ${noun}: ${activeLabel}`}
          title=${`Filter ${noun}: ${activeLabel}`}
        >
          <omb-icon name="funnel" aria-hidden="true"></omb-icon>
          ${current.kind === "all" ? nothing : html`<span class="max-w-28 truncate">${activeLabel}</span>`}
        </summary>
        <ul
          class="dropdown-content menu menu-sm z-30 mt-1 w-64 max-h-96 overflow-y-auto rounded-box border border-base-300 bg-base-100 p-2 shadow-lg"
          aria-label=${`Filter ${noun}`}
        >
          <li class="menu-title">${side === "source" ? "Sources" : "Destinations"}</li>
          ${option("All devices", { kind: "all" }, current.kind === "all")}
          ${option("Mounted devices", { kind: "mounted" }, current.kind === "mounted")}
          <li class="menu-title mt-1 border-t border-base-300">
            Specific ${side === "source" ? "source" : "destination"} device
          </li>
          ${devices.map((device) => option(device.name, { kind: "device", deviceId: device.id }, current.kind === "device" && current.deviceId === device.id))}
          ${side === "destination" ? html`<li class="menu-title font-normal whitespace-normal">App destinations stay visible.</li>` : nothing}
        </ul>
      </details>
    `;
  }

  #heading(
    icon: string,
    label: string,
    side: "source" | "destination",
    count: number,
    add: () => void,
    addLabel: string,
  ) {
    return html`
      <div class="flex flex-wrap items-center justify-between gap-3">
        <h2
          class="flex items-center gap-2 text-sm font-semibold tracking-widest uppercase text-base-content/60"
        >
          <omb-icon name=${icon}></omb-icon>${label}<span class="badge badge-sm badge-neutral">${count}</span>
        </h2>
        <div class="ml-auto flex items-center gap-2">
          ${this.#filterMenu(side)}
          <button class="btn btn-sm btn-outline border-base-300 gap-1.5 font-normal" @click=${add}>
            <omb-icon name="plus"></omb-icon>${addLabel}
          </button>
        </div>
      </div>
    `;
  }

  override render() {
    const { snapshot, space } = this.store;
    if (!snapshot || !space) return nothing;
    const allSources = spaceSources(snapshot, space.id);
    const allDestinations = spaceDestinations(snapshot, space.id);
    const sources = filterSources(allSources, this.store.status, this.sourceFilter);
    const destinations = filterDestinations(allDestinations, this.store.status, this.destinationFilter);
    const showMountedFirst = snapshot.settings.show_mounted_devices_first !== false;
    const orderedSources = showMountedFirst
      ? mountedFirst(sources, this.store.volumes, (source) => source.device_id)
      : sources;
    const orderedDestinations = showMountedFirst
      ? mountedFirstInSlots(destinations, this.store.volumes, (destination) =>
          (destination.kind ?? "folder") === "app" ? null : destination.device_id,
        )
      : destinations;
    return html`
      <div
        class="relative grid grid-cols-[minmax(300px,30rem)_minmax(8rem,1fr)_minmax(26rem,48rem)] gap-y-5 h-full content-start"
        data-flow-board
      >
        <div class="col-start-1">
          ${this.#heading("log-in", "Sources", "source", sources.length, () => this.store.open({ type: "source-settings", sourceId: null }), "Add source")}
        </div>
        <div class="col-start-3">
          ${this.#heading("log-out", "Destinations", "destination", destinations.length, () => this.store.open({ type: "destination-settings", destinationId: null }), "Add destination")}
        </div>
        <div class="col-start-1 flex flex-col gap-5">
          ${orderedSources.map((s) => html`<omb-source-card data-omb-block .source=${s}></omb-source-card>`)}
          ${sources.length === 0 && allSources.length > 0 ? html`<p role="status" class="text-sm text-base-content/60">No sources match this filter.</p>` : nothing}
          <omb-new-volumes data-omb-block></omb-new-volumes>
        </div>
        <div class="col-start-3 flex flex-col gap-5">
          ${orderedDestinations.map((d) => html`<omb-destination-card data-omb-block .destination=${d}></omb-destination-card>`)}
          ${
            allDestinations.length === 0
              ? html`<button
                  class="card border border-dashed border-base-300 p-5 text-sm text-base-content/60 hover:border-primary"
                  @click=${() => this.store.open({ type: "destination-settings", destinationId: null })}
                >
                  Add an SSD, NAS or folder where files should be copied
                </button>`
              : nothing
          }
          ${destinations.length === 0 && allDestinations.length > 0 ? html`<p role="status" class="text-sm text-base-content/60">No destinations match this filter.</p>` : nothing}
        </div>
        <omb-flow-canvas class="absolute inset-0 pointer-events-none"></omb-flow-canvas>
      </div>
    `;
  }
}
