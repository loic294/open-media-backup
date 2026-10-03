import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import type { PreviewAppMediaType, ThemePreference } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { previewAppChoiceLabel, previewApps } from "../../utils/preview-apps";
import { DialogBase } from "./dialog-base";

const THEMES: [ThemePreference, string, string][] = [
  ["system", "System", "monitor"],
  ["light", "Light", "sun"],
  ["dark", "Dark", "moon"],
];

@customElement("omb-app-settings-dialog")
export class OmbAppSettingsDialog extends DialogBase<Extract<DialogRequest, { type: "app-settings" }>> {
  async #choosePreviewApp(type: PreviewAppMediaType) {
    const s = this.store.snapshot?.settings;
    const os = this.store.snapshot?.computer.os ?? "";
    if (!s) return;
    const app = await this.store.backend.pickPreviewApp(os);
    if (!app) return;
    await this.store.saveSettings({ preview_apps: { ...previewApps(s), [type]: app } });
  }

  async #useDefault(type: PreviewAppMediaType) {
    const s = this.store.snapshot?.settings;
    if (!s) return;
    await this.store.saveSettings({ preview_apps: { ...previewApps(s), [type]: null } });
  }

  override render() {
    const s = this.store.snapshot?.settings;
    const computer = this.store.snapshot?.computer;
    if (!s) return nothing;
    const apps = previewApps(s);
    const os = computer?.os;
    const previewRows: [PreviewAppMediaType, string][] = [
      ["photos", "Photos"],
      ["videos", "Videos"],
    ];
    const body = html`
      <div class="flex flex-col gap-6">
        <section>
          <h4 class="font-medium mb-2">Appearance</h4>
          <div class="join">
            ${THEMES.map(
              ([value, label, icon]) =>
                html`<button
                  class="btn join-item gap-2 ${s.theme === value ? "btn-primary" : ""}"
                  @click=${() => this.store.saveSettings({ theme: value })}
                >
                  <omb-icon name=${icon}></omb-icon>${label}
                </button>`,
            )}
          </div>
        </section>
        <section class="flex flex-col gap-2">
          <h4 class="font-medium">Sync</h4>
          <label class="flex items-center gap-3">
            <input
              type="checkbox"
              class="toggle toggle-primary"
              .checked=${s.auto_sync}
              @change=${(e: Event) => this.store.saveSettings({ auto_sync: (e.target as HTMLInputElement).checked })}
            />
            <span class="text-sm">Sync with peers automatically</span>
          </label>
          <label class="flex items-center gap-3 text-sm">
            Listen port
            <input
              type="number"
              class="input input-sm w-28"
              .value=${String(s.sync_port)}
              @change=${(e: Event) => this.store.saveSettings({ sync_port: Number((e.target as HTMLInputElement).value) || 47821 })}
            />
            <span class="text-base-content/50">applies after restart</span>
          </label>
        </section>
        <section class="flex flex-col gap-3">
          <h4 class="font-medium">Preview apps</h4>
          <div class="flex flex-col gap-2">
            ${previewRows.map(
              ([type, label]) =>
                html`<div class="grid grid-cols-[4.5rem_1fr_auto] items-center gap-2 text-sm">
                  <span class="font-medium text-base-content/70">${label}</span>
                  <div class="input input-sm w-full items-center text-base-content/80">
                    ${previewAppChoiceLabel(s, os, type)}
                  </div>
                  <div class="flex gap-2">
                    ${
                      apps[type]
                        ? html`<button class="btn btn-ghost btn-sm" @click=${() => this.#useDefault(type)}>
                            Use default
                          </button>`
                        : nothing
                    }
                    <button class="btn btn-sm" @click=${() => this.#choosePreviewApp(type)}>
                      Choose app…
                    </button>
                  </div>
                </div>`,
            )}
          </div>
          <p class="text-xs text-base-content/60">
            Used when double-clicking a thumbnail or choosing Open in app. Set separately on each computer
            (macOS / Windows).
          </p>
        </section>
        ${
          computer
            ? html`<section class="text-sm text-base-content/60">
                This computer: <b>${computer.name}</b> (${computer.os}) ·
                <span class="font-mono text-xs">${computer.id}</span>
              </section>`
            : nothing
        }
      </div>
    `;
    return html`<omb-modal
      size="sm"
      heading="Settings"
      icon="settings"
      @close=${this.onClosed}
      .body=${body}
    ></omb-modal>`;
  }
}
