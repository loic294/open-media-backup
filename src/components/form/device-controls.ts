import { html, nothing } from "lit";
import type { Device, DeviceKind, DeviceRole, Volume } from "../../api/types";
import { formatBytes } from "../../utils/format";
import { DEVICE_KIND_LABEL } from "../ui/device-icon";

export const ROLE_INFO: Record<DeviceRole, string> = {
  original: "Where media is born (memory card, camera). Can be wiped once safely copied.",
  temporary: "A copy on the road (external SSD). Not counted as safe; can be wiped later.",
  final: "Long-term storage (NAS, archive drive). Counts as a safe copy and is never wiped.",
};

export function deviceKindRole(device: Device, onChange: (patch: Partial<Device>) => void) {
  return html`
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
      <fieldset class="fieldset">
        <legend class="fieldset-legend">Type</legend>
        <select
          aria-label="Device type"
          class="select w-full"
          .value=${device.kind}
          @change=${(e: Event) => onChange({ kind: (e.target as HTMLSelectElement).value as DeviceKind })}
        >
          ${Object.entries(DEVICE_KIND_LABEL).map(([kind, label]) => html`<option value=${kind} ?selected=${kind === device.kind}>${label}</option>`)}
        </select>
      </fieldset>
      <fieldset class="fieldset">
        <legend class="fieldset-legend">Role</legend>
        <select
          aria-label="Device role"
          class="select w-full"
          .value=${device.role}
          @change=${(e: Event) => onChange({ role: (e.target as HTMLSelectElement).value as DeviceRole })}
        >
          ${(["original", "temporary", "final"] as const).map((role) => html`<option value=${role} ?selected=${role === device.role}>${role[0].toUpperCase() + role.slice(1)}</option>`)}
        </select>
      </fieldset>
    </div>
    <p class="text-xs text-base-content/60">${ROLE_INFO[device.role]}</p>
  `;
}

export function deviceVolumes(
  volumes: Volume[],
  selected: string,
  onPick: (volume: Volume) => void,
  radioName = "omb-volume",
) {
  if (!volumes.length) return nothing;
  return html`<div class="flex flex-col gap-1">
    ${volumes.map(
      (v) =>
        html`<label
          class="flex items-center gap-3 rounded-field border border-base-300 px-3 py-2 cursor-pointer hover:bg-base-200 ${selected === v.mount_path ? "border-primary bg-primary/5" : ""}"
        >
          <input
            type="radio"
            class="radio radio-sm radio-primary"
            name=${radioName}
            .checked=${selected === v.mount_path}
            @change=${() => onPick(v)}
          />
          <omb-icon name=${v.removable ? "card-sim" : "hard-drive"}></omb-icon>
          <span class="flex-1 min-w-0 text-sm"
            ><b>${v.name}</b>
            <span class="text-base-content/50 font-mono break-all">${v.mount_path}</span></span
          >
          <span class="text-xs text-base-content/50">${formatBytes(v.total_bytes)}</span>
        </label>`,
    )}
  </div>`;
}
