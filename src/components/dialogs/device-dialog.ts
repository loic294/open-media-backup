import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { Device } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { newDevice } from "../../state/factories";
import { mappingFor } from "../../state/selectors";
import { formatBytes } from "../../utils/format";
import { deviceKindRole, deviceVolumes } from "../form/device-controls";
import { DialogBase } from "./dialog-base";

@customElement("omb-device-dialog")
export class OmbDeviceDialog extends DialogBase<Extract<DialogRequest, { type: "device-settings" }>> {
  @state() private draft!: Device;
  @state() private path = "";
  @state() private busy = false;
  private initialPath = "";

  override connectedCallback(): void {
    super.connectedCallback();
    const snapshot = this.store.snapshot;
    const device = snapshot?.devices.find((d) => d.id === this.request.deviceId);
    this.draft = device ? structuredClone(device) : newDevice("", "other", "original");
    this.path = device && snapshot ? (mappingFor(snapshot, device.id)?.root_path ?? "") : "";
    this.initialPath = this.path;
  }

  async #chooseFolder() {
    try {
      const path = await this.store.backend.pickFolder();
      if (path) this.path = path;
    } catch (error) {
      this.store.toast("error", `Could not choose a device location: ${error}`);
    }
  }

  async #save() {
    if (this.busy || !this.draft.name.trim()) return;
    if (this.request.deviceId && !this.store.snapshot?.devices.some((d) => d.id === this.request.deviceId)) {
      this.store.toast("error", "This device was removed. Close this editor and add a new device.");
      return;
    }
    this.busy = true;
    try {
      // Merge only editable metadata so a refreshed probe/sync cannot lose identity hints.
      const current = this.store.snapshot?.devices.find((d) => d.id === this.draft.id);
      const device = {
        ...(current ?? this.draft),
        name: this.draft.name.trim(),
        format_name: this.draft.format_name?.trim() ?? "",
        description: this.draft.description,
        kind: this.draft.kind,
        role: this.draft.role,
      };
      if (this.path && !this.request.deviceId) {
        await this.store.backend.registerDevice(this.path, device);
      } else {
        await this.store.backend.saveEntity("device", device);
        if (this.path && this.path !== this.initialPath) {
          await this.store.backend.relinkDevice(device.id, this.path);
        }
      }
      await this.store.reloadSnapshot();
      await this.store.refreshVolumes();
      this.dismiss();
    } catch (error) {
      this.store.toast("error", `Could not save device: ${error}`);
    } finally {
      this.busy = false;
    }
  }

  override render() {
    const snapshot = this.store.snapshot;
    if (!snapshot || !this.draft) return nothing;
    const d = this.draft;
    const set = (patch: Partial<Device>) => (this.draft = { ...this.draft, ...patch });
    const elsewhere = snapshot.mappings.filter(
      (m) => m.device_id === d.id && m.computer_id !== snapshot.computer.id,
    );
    const body = html`
      <div class="flex flex-col gap-4">
        <fieldset class="fieldset" ?disabled=${this.busy}>
          <legend class="fieldset-legend">Device name</legend>
          <input
            aria-label="Device name"
            class="input w-full"
            required
            .value=${d.name}
            @input=${(e: Event) => set({ name: (e.target as HTMLInputElement).value })}
          />
          <p class="label whitespace-normal">
            Shared by every source and destination using this physical device, and synced to your other
            computers.
          </p>
        </fieldset>
        <fieldset class="fieldset" ?disabled=${this.busy}>
          <legend class="fieldset-legend">Name when formatting</legend>
          <input
            aria-label="Name when formatting"
            aria-describedby="omb-device-format-name-help"
            class="input w-full"
            placeholder="Use device name"
            .value=${d.format_name ?? ""}
            @input=${(e: Event) => set({ format_name: (e.target as HTMLInputElement).value })}
          />
          <p id="omb-device-format-name-help" class="label whitespace-normal">
            Volume name used only by Quick format (exFAT). Leave blank to use the device name. The formatter
            keeps letters A-Z, numbers, spaces and underscores, converts to uppercase, and uses the first 11
            characters (or MEDIA if none remain). Synced to your other computers; saving does not rename or
            format the mounted device.
          </p>
        </fieldset>
        <fieldset class="fieldset" ?disabled=${this.busy}>
          <legend class="fieldset-legend">Description</legend>
          <input
            aria-label="Device description"
            class="input w-full"
            .value=${d.description}
            @input=${(e: Event) => set({ description: (e.target as HTMLInputElement).value })}
          />
          ${deviceKindRole(d, set)}
        </fieldset>
        <fieldset class="fieldset" ?disabled=${this.busy}>
          <legend class="fieldset-legend">Location on ${snapshot.computer.name}</legend>
          ${deviceVolumes(
            this.store.volumes.filter((v) => !v.device_id || v.device_id === d.id),
            this.path,
            (v) => {
              this.path = v.mount_path;
              if (!d.name.trim()) set({ name: v.name });
            },
            "omb-device-editor-volume",
          )}
          <div class="flex items-center gap-2">
            <button type="button" class="btn btn-sm" @click=${() => this.#chooseFolder()}>
              Choose folder...
            </button>
            <span class="text-xs font-mono break-all">${this.path || "Not located on this computer"}</span>
          </div>
          <p class="label whitespace-normal">
            Location is optional. Register an offline device now and locate it later.
          </p>
          ${elsewhere.map((m) => html`<p class="text-xs text-base-content/60">On ${snapshot.computers.find((c) => c.id === m.computer_id)?.name ?? m.computer_id}: <span class="font-mono">${m.root_path}</span></p>`)}
        </fieldset>
        ${
          this.request.deviceId
            ? html` <section class="text-xs text-base-content/60 break-all">
                <h4 class="font-medium mb-1">Detected hardware (read-only)</h4>
                <div>Serial: ${d.hw_serial ?? "Not detected"}</div>
                <div>Volume UUID: ${d.volume_uuid ?? "Not detected"}</div>
                <div>
                  Capacity: ${d.capacity_bytes === null ? "Not detected" : formatBytes(d.capacity_bytes)}
                </div>
              </section>`
            : nothing
        }
      </div>
    `;
    return html`<omb-modal
      heading=${this.request.deviceId ? "Edit device" : "Add device"}
      icon="hard-drive"
      .closeable=${!this.busy}
      @close=${this.onClosed}
      .body=${body}
      .actions=${html`
        <button class="btn btn-ghost" ?disabled=${this.busy} @click=${() => this.dismiss()}>Cancel</button>
        <button class="btn btn-primary" ?disabled=${this.busy || !d.name.trim()} @click=${() => this.#save()}>
          ${this.busy ? html`<span class="loading loading-spinner loading-xs"></span>` : nothing}
          ${this.request.deviceId ? "Save device" : "Add device"}
        </button>
      `}
    ></omb-modal>`;
  }
}
