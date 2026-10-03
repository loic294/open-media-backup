import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import type { ThemePreference } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { DialogBase } from "./dialog-base";

const THEMES: [ThemePreference, string, string][] = [
  ["system", "System", "monitor"],
  ["light", "Light", "sun"],
  ["dark", "Dark", "moon"],
];

@customElement("omb-app-settings-dialog")
export class OmbAppSettingsDialog extends DialogBase<Extract<DialogRequest, { type: "app-settings" }>> {
  override render() {
    const s = this.store.snapshot?.settings;
    const computer = this.store.snapshot?.computer;
    if (!s) return nothing;
    const body = html`
      <div class="flex flex-col gap-6">
        <section>
          <h4 class="font-medium mb-2">Appearance</h4>
          <div class="join">
            ${THEMES.map(
              ([value, label, icon]) => html`<button class="btn join-item gap-2 ${s.theme === value ? "btn-primary" : ""}" @click=${() => this.store.saveSettings({ theme: value })}>
                <omb-icon name=${icon}></omb-icon>${label}
              </button>`,
            )}
          </div>
        </section>
        <section class="flex flex-col gap-2">
          <h4 class="font-medium">Sync</h4>
          <label class="flex items-center gap-3">
            <input type="checkbox" class="toggle toggle-primary" .checked=${s.auto_sync} @change=${(e: Event) => this.store.saveSettings({ auto_sync: (e.target as HTMLInputElement).checked })} />
            <span class="text-sm">Sync with peers automatically</span>
          </label>
          <label class="flex items-center gap-3 text-sm">
            Listen port
            <input type="number" class="input input-sm w-28" .value=${String(s.sync_port)} @change=${(e: Event) => this.store.saveSettings({ sync_port: Number((e.target as HTMLInputElement).value) || 47821 })} />
            <span class="text-base-content/50">applies after restart</span>
          </label>
        </section>
        ${computer
          ? html`<section class="text-sm text-base-content/60">
              This computer: <b>${computer.name}</b> (${computer.os}) · <span class="font-mono text-xs">${computer.id}</span>
            </section>`
          : nothing}
      </div>
    `;
    return html`<omb-modal size="sm" heading="Settings" icon="settings" @close=${this.onClosed} .body=${body}></omb-modal>`;
  }
}
