import { html, nothing } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import type { Device, DeviceKind, DeviceRole, Volume } from "../../api/types";
import { newDevice } from "../../state/factories";
import { deviceById, mappingFor } from "../../state/selectors";
import { formatBytes } from "../../utils/format";
import { DEVICE_ICON, DEVICE_KIND_LABEL } from "../ui/device-icon";
import { OmbElement } from "../ui/omb-element";

export const ROLE_INFO: Record<DeviceRole, string> = {
  original: "Where media is born (memory card, camera). Can be wiped once safely copied.",
  temporary: "A copy on the road (external SSD). Not counted as safe; can be wiped later.",
  final: "Long-term storage (NAS, archive drive). Counts as a safe copy and is never wiped.",
};

/**
 * Pick an existing device or register a new one from a mounted volume or folder.
 * Name/kind/role edits are saved immediately (synced to peers). Emits "device-change".
 */
@customElement("omb-device-field")
export class OmbDeviceField extends OmbElement {
  @property() deviceId: string | null = null;
  @property() defaultRole: DeviceRole = "original";
  @property() mountPath = "";
  @state() private creating = false;
  @state() private draft: Device = newDevice("", "sd_card", "original");
  @state() private draftPath = "";
  @state() private busy = false;

  override willUpdate(changed: Map<string, unknown>): void {
    if (changed.has("deviceId") || changed.has("mountPath")) {
      this.creating = !this.deviceId;
      if (this.creating) this.#resetDraft();
    }
  }

  #resetDraft() {
    const volume = this.store.volumes.find((v) => v.mount_path === this.mountPath);
    const kind: DeviceKind = this.defaultRole === "final" ? "nas" : this.defaultRole === "temporary" ? "ssd" : "sd_card";
    this.draft = newDevice(volume?.name ?? "", kind, this.defaultRole);
    this.draftPath = this.mountPath;
  }

  #emit(id: string) {
    this.deviceId = id;
    this.dispatchEvent(new CustomEvent("device-change", { detail: id }));
  }

  async #run(fn: () => Promise<void>) {
    this.busy = true;
    try {
      await fn();
    } catch (e) {
      this.store.toast("error", String(e));
    } finally {
      this.busy = false;
    }
  }

  #create() {
    const device = { ...this.draft, name: this.draft.name.trim() };
    return this.#run(async () => {
      await this.store.backend.registerDevice(this.draftPath, device);
      await this.store.reloadSnapshot();
      this.creating = false;
      this.#emit(device.id);
    });
  }

  #relink(deviceId: string, path: string) {
    return this.#run(async () => {
      await this.store.backend.relinkDevice(deviceId, path);
      await this.store.reloadSnapshot();
      this.store.refreshStatus();
    });
  }

  async #choose(apply: (path: string) => void | Promise<void>) {
    const path = await this.store.backend.pickFolder();
    if (path) await apply(path);
  }

  #update(device: Device, patch: Partial<Device>) {
    void this.store.save("device", { ...device, ...patch });
  }

  #volumeOptions(selected: string, onPick: (v: Volume) => void, filter: (v: Volume) => boolean = () => true) {
    const volumes = this.store.volumes.filter(filter);
    if (!volumes.length) return nothing;
    return html`<div class="flex flex-col gap-1">
      ${volumes.map(
        (v) => html`<label class="flex items-center gap-3 rounded-field border border-base-300 px-3 py-2 cursor-pointer hover:bg-base-200 ${selected === v.mount_path ? "border-primary bg-primary/5" : ""}">
          <input type="radio" class="radio radio-sm radio-primary" name="omb-volume" .checked=${selected === v.mount_path} @change=${() => onPick(v)} />
          <omb-icon name=${v.removable ? "card-sim" : "hard-drive"}></omb-icon>
          <span class="flex-1 text-sm"><b>${v.name}</b> <span class="text-base-content/50 font-mono">${v.mount_path}</span></span>
          <span class="text-xs text-base-content/50">${formatBytes(v.total_bytes)}</span>
        </label>`,
      )}
    </div>`;
  }

  #kindRole(device: Device, onChange: (patch: Partial<Device>) => void) {
    return html`
      <div class="grid grid-cols-2 gap-3">
        <fieldset class="fieldset">
          <legend class="fieldset-legend">Type</legend>
          <select class="select w-full" .value=${device.kind} @change=${(e: Event) => onChange({ kind: (e.target as HTMLSelectElement).value as DeviceKind })}>
            ${Object.entries(DEVICE_KIND_LABEL).map(([k, label]) => html`<option value=${k} ?selected=${k === device.kind}>${label}</option>`)}
          </select>
        </fieldset>
        <fieldset class="fieldset">
          <legend class="fieldset-legend">Role</legend>
          <select class="select w-full" .value=${device.role} @change=${(e: Event) => onChange({ role: (e.target as HTMLSelectElement).value as DeviceRole })}>
            ${(["original", "temporary", "final"] as const).map((r) => html`<option value=${r} ?selected=${r === device.role}>${r[0].toUpperCase() + r.slice(1)}</option>`)}
          </select>
        </fieldset>
      </div>
      <p class="text-xs text-base-content/60 -mt-1">${ROLE_INFO[device.role]}</p>
    `;
  }

  #renderNew() {
    const d = this.draft;
    const set = (patch: Partial<Device>) => (this.draft = { ...d, ...patch });
    return html`
      <div class="flex flex-col gap-3 rounded-box border border-base-300 p-4">
        <div class="text-sm font-medium">Where is it on this computer?</div>
        ${this.#volumeOptions(this.draftPath, (v) => ((this.draftPath = v.mount_path), set({ name: d.name || v.name })), (v) => !v.device_id)}
        <div class="flex items-center gap-2">
          <button type="button" class="btn btn-sm gap-1.5" @click=${() => this.#choose((p) => void (this.draftPath = p))}><omb-icon name="folder-open"></omb-icon>Choose folder…</button>
          <span class="text-xs font-mono text-base-content/60 truncate">${this.draftPath}</span>
        </div>
        <fieldset class="fieldset">
          <legend class="fieldset-legend">Device name</legend>
          <input class="input w-full" .value=${d.name} placeholder="A7IV · Card 1" @input=${(e: Event) => set({ name: (e.target as HTMLInputElement).value })} />
          <p class="label">Shared with your other computers, so the same card has the same name everywhere.</p>
        </fieldset>
        ${this.#kindRole(d, set)}
        <div class="flex justify-end gap-2">
          ${this.store.snapshot?.devices.length ? html`<button type="button" class="btn btn-ghost btn-sm" @click=${() => (this.creating = false)}>Use existing device</button>` : nothing}
          <button type="button" class="btn btn-primary btn-sm" ?disabled=${!d.name.trim() || !this.draftPath || this.busy} @click=${() => this.#create()}>
            ${this.busy ? html`<span class="loading loading-spinner loading-xs"></span>` : nothing}Register device
          </button>
        </div>
      </div>
    `;
  }

  #renderExisting() {
    const snapshot = this.store.snapshot!;
    const device = this.deviceId ? deviceById(snapshot, this.deviceId) : undefined;
    const mapping = device ? mappingFor(snapshot, device.id) : undefined;
    const elsewhere = device ? snapshot.mappings.filter((m) => m.device_id === device.id && m.computer_id !== snapshot.computer.id) : [];
    return html`
      <div class="flex flex-col gap-3">
        <select
          class="select w-full"
          @change=${(e: Event) => {
            const v = (e.target as HTMLSelectElement).value;
            if (v === "__new") {
              this.creating = true;
              this.#resetDraft();
            } else this.#emit(v);
          }}
        >
          ${!device ? html`<option selected disabled>Choose a device…</option>` : nothing}
          ${snapshot.devices.map((d) => html`<option value=${d.id} ?selected=${d.id === device?.id}>${d.name} — ${DEVICE_KIND_LABEL[d.kind]}, ${d.role}</option>`)}
          <option value="__new">＋ New device…</option>
        </select>
        ${device
          ? html`
              <div class="flex flex-col gap-3 rounded-box border border-base-300 p-4">
                <div class="flex items-center gap-3">
                  <omb-icon name=${DEVICE_ICON[device.kind]} class="size-5 text-primary"></omb-icon>
                  <input class="input input-sm flex-1 font-semibold" .value=${device.name} @change=${(e: Event) => this.#update(device, { name: (e.target as HTMLInputElement).value.trim() || device.name })} />
                </div>
                <input class="input input-sm w-full" placeholder="Description, e.g. Samsung T7" .value=${device.description} @change=${(e: Event) => this.#update(device, { description: (e.target as HTMLInputElement).value })} />
                ${this.#kindRole(device, (patch) => this.#update(device, patch))}
                <div class="text-sm font-medium mt-1">Location on ${snapshot.computer.name}</div>
                <div class="flex items-center gap-2">
                  <span class="status ${mapping ? "status-success" : "status-neutral"}"></span>
                  <span class="flex-1 font-mono text-xs truncate">${mapping?.root_path ?? "Not located on this computer yet"}</span>
                  <button type="button" class="btn btn-xs gap-1" ?disabled=${this.busy} @click=${() => this.#choose((p) => this.#relink(device.id, p))}>
                    <omb-icon name="folder-open" class="size-3"></omb-icon>${mapping ? "Change…" : "Locate…"}
                  </button>
                </div>
                ${this.#volumeOptions(mapping?.root_path ?? "", (v) => void this.#relink(device.id, v.mount_path), (v) => !v.device_id || v.device_id === device.id)}
                ${elsewhere.map(
                  (m) => html`<div class="text-xs text-base-content/60">
                    On ${snapshot.computers.find((c) => c.id === m.computer_id)?.name ?? m.computer_id}: <span class="font-mono">${m.root_path}</span>
                  </div>`,
                )}
              </div>
            `
          : nothing}
      </div>
    `;
  }

  override render() {
    if (!this.store.snapshot) return nothing;
    return this.creating ? this.#renderNew() : this.#renderExisting();
  }
}
