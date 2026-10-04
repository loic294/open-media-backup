import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import type { Device } from "../../api/types";
import { mappingFor } from "../../state/selectors";
import { DEVICE_ICON, DEVICE_KIND_LABEL } from "../ui/device-icon";
import { OmbElement } from "../ui/omb-element";

@customElement("omb-devices-settings")
export class OmbDevicesSettings extends OmbElement {
  #usage(deviceId: string) {
    const snapshot = this.store.snapshot!;
    return {
      sources: snapshot.sources.filter((s) => s.device_id === deviceId).length,
      destinations: snapshot.destinations.filter((d) => d.device_id === deviceId).length,
    };
  }

  #remove(device: Device) {
    const { sources, destinations } = this.#usage(device.id);
    this.store.open({
      type: "confirm",
      title: `Remove ${device.name}?`,
      message: `This clears the device assignment from ${sources} source(s) and ${destinations} destination(s) across all spaces and removes its locations on every computer. Tasks and connections remain, but affected transfers cannot run until you select another device. Physical files and backup history are not deleted. This change syncs to your other computers.`,
      confirmLabel: "Remove device",
      danger: true,
      onConfirm: async () => {
        await this.store.removeDevice(device.id);
      },
    });
  }

  override render() {
    const snapshot = this.store.snapshot;
    if (!snapshot) return nothing;
    const devices = [...snapshot.devices].sort((a, b) => a.name.localeCompare(b.name));
    return html`
      <section class="flex flex-col gap-4">
        <label class="flex items-start gap-3 cursor-pointer rounded-box border border-base-300 p-3">
          <input
            type="checkbox"
            class="toggle toggle-primary mt-0.5"
            aria-label="Show mounted devices first"
            .checked=${snapshot.settings.show_mounted_devices_first ?? true}
            @change=${(event: Event) =>
              this.store.saveSettings({
                show_mounted_devices_first: (event.target as HTMLInputElement).checked,
              })}
          />
          <span>
            <span class="font-medium">Show mounted devices first</span>
            <span class="block text-sm text-base-content/60">
              Move devices connected to this computer to the top of source and destination cards and device
              lists.
            </span>
          </span>
        </label>
        <div class="flex items-start justify-between gap-3">
          <div>
            <h4 class="font-medium">Devices</h4>
            <p class="text-sm text-base-content/60">
              All registered devices across your spaces and computers, including offline devices.
            </p>
          </div>
          <button
            type="button"
            class="btn btn-primary btn-sm shrink-0"
            @click=${() => this.store.open({ type: "device-settings", deviceId: null })}
          >
            Add device
          </button>
        </div>
        ${
          devices.length
            ? html`<ul class="list">
                ${devices.map((d) => {
                  const mapping = mappingFor(snapshot, d.id);
                  const usage = this.#usage(d.id);
                  return html`<li class="list-row border-b border-base-300" data-device-id=${d.id}>
                    <omb-icon name=${DEVICE_ICON[d.kind]}></omb-icon>
                    <div class="min-w-0">
                      <div class="font-semibold break-words">${d.name}</div>
                      <div class="text-sm text-base-content/60">${DEVICE_KIND_LABEL[d.kind]} / ${d.role}</div>
                      <div class="text-xs font-mono break-all">
                        ${mapping?.root_path ?? "Not located on this computer"}
                      </div>
                      <div class="text-xs text-base-content/60">
                        ${usage.sources} source(s), ${usage.destinations} destination(s)
                      </div>
                    </div>
                    <div class="flex flex-col sm:flex-row gap-1">
                      <button
                        type="button"
                        class="btn btn-sm btn-ghost"
                        aria-label=${`Edit ${d.name}`}
                        @click=${() => this.store.open({ type: "device-settings", deviceId: d.id })}
                      >
                        Edit
                      </button>
                      <button
                        type="button"
                        class="btn btn-sm btn-ghost text-error"
                        aria-label=${`Remove ${d.name}`}
                        @click=${() => this.#remove(d)}
                      >
                        Remove
                      </button>
                    </div>
                  </li>`;
                })}
              </ul>`
            : html`<p class="text-sm text-base-content/60 py-6">
                No devices yet. Add a device to start, even if it is offline.
              </p>`
        }
      </section>
    `;
  }
}
