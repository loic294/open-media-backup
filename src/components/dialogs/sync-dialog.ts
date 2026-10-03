import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { PeerStatus } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { formatAgo, initials, percent } from "../../utils/format";
import { PEER_LABEL, PEER_TONE } from "../top-bar/peer-tone";
import { DialogBase } from "./dialog-base";

const BADGE = { up_to_date: "badge-success", syncing: "badge-info", idle: "badge-ghost", offline: "badge-ghost", error: "badge-error" } as const;

@customElement("omb-sync-dialog")
export class OmbSyncDialog extends DialogBase<Extract<DialogRequest, { type: "device-sync" }>> {
  @state() private address = "";
  @state() private token = "";
  @state() private busy = false;

  async #run(fn: () => Promise<void>, ok?: string) {
    this.busy = true;
    try {
      await fn();
      if (ok) this.store.toast("success", ok);
    } catch (e) {
      this.store.toast("error", String(e));
    } finally {
      this.busy = false;
    }
  }

  #copy(text: string) {
    void navigator.clipboard?.writeText(text).then(() => this.store.toast("info", "Copied"));
  }

  #peer(p: PeerStatus) {
    return html`<li class="flex items-center gap-3 py-3">
      <span class="grid place-items-center size-9 rounded-full text-xs font-bold ${PEER_TONE[p.state]}">${initials(p.name)}</span>
      <div class="flex-1 min-w-0">
        <div class="flex items-center gap-2">
          <span class="font-medium truncate">${p.name}</span>
          <span class="badge badge-sm badge-soft ${BADGE[p.state]}">${PEER_LABEL[p.state]}</span>
        </div>
        <div class="text-xs text-base-content/60 truncate">
          <span class="font-mono">${p.address}</span> · synced ${formatAgo(p.last_synced)}${p.latency_ms != null ? ` · ${p.latency_ms} ms` : ""}
        </div>
        ${p.state === "syncing" ? html`<progress class="progress progress-info h-1 w-full" value=${percent(p.progress, 1)} max="100"></progress>` : nothing}
        ${p.message ? html`<div class="text-xs ${p.state === "error" ? "text-error" : "text-base-content/60"}">${p.message}</div>` : nothing}
      </div>
      <button
        class="btn btn-ghost btn-sm btn-square"
        title="Remove peer"
        @click=${() =>
          this.store.open({
            type: "confirm",
            title: `Remove ${p.name}?`,
            message: "This computer stops syncing with it. Nothing is deleted from either catalog.",
            confirmLabel: "Remove",
            danger: true,
            onConfirm: () => this.store.backend.removePeer(p.id),
          })}
      >
        <omb-icon name="trash"></omb-icon>
      </button>
    </li>`;
  }

  override render() {
    const sync = this.store.sync;
    const settings = this.store.snapshot?.settings;
    const body = html`
      <div class="flex flex-col gap-5">
        <section class="rounded-box bg-base-200 p-4 flex flex-col gap-2">
          <h4 class="font-medium">This computer</h4>
          <p class="text-xs text-base-content/60">On another computer, add this address and pairing token. Use the Netbird IP so peers can reach each other anywhere.</p>
          ${(
            [
              ["Address", sync?.listen_address ?? "—"],
              ["Token", sync?.token ?? "—"],
            ] as const
          ).map(
            ([label, value]) => html`<div class="flex items-center gap-2">
              <span class="w-16 text-sm text-base-content/60">${label}</span>
              <code class="flex-1 font-mono text-sm truncate">${value}</code>
              <button class="btn btn-ghost btn-xs btn-square" title="Copy" @click=${() => this.#copy(value)}><omb-icon name="copy"></omb-icon></button>
            </div>`,
          )}
        </section>
        <section>
          <div class="flex items-center justify-between">
            <h4 class="font-medium">Peers</h4>
            <button class="btn btn-sm gap-1.5" ?disabled=${this.busy} @click=${() => this.#run(() => this.store.backend.syncNow())}>
              <omb-icon name="refresh-cw" class=${sync?.syncing ? "animate-spin" : ""}></omb-icon>Sync now
            </button>
          </div>
          ${sync?.peers.length ? html`<ul class="divide-y divide-base-300">${sync.peers.map((p) => this.#peer(p))}</ul>` : html`<p class="text-sm text-base-content/60 py-4">No peers yet.</p>`}
        </section>
        <section class="flex flex-col gap-2">
          <h4 class="font-medium">Add a peer</h4>
          <div class="flex gap-2">
            <input class="input input-sm flex-1 font-mono" placeholder="10.0.0.21:47821" .value=${this.address} @input=${(e: Event) => (this.address = (e.target as HTMLInputElement).value.trim())} />
            <input class="input input-sm flex-1 font-mono" placeholder="Peer token" .value=${this.token} @input=${(e: Event) => (this.token = (e.target as HTMLInputElement).value.trim())} />
            <button
              class="btn btn-sm btn-primary"
              ?disabled=${!this.address || !this.token || this.busy}
              @click=${() =>
                this.#run(async () => {
                  await this.store.backend.addPeer(this.address, this.token);
                  this.address = this.token = "";
                }, "Peer added")}
            >
              <omb-icon name="plus"></omb-icon>Add
            </button>
          </div>
        </section>
        ${settings
          ? html`<label class="flex items-center gap-3">
              <input type="checkbox" class="toggle toggle-primary" .checked=${settings.auto_sync} @change=${(e: Event) => this.store.saveSettings({ auto_sync: (e.target as HTMLInputElement).checked })} />
              <span class="text-sm">Sync automatically every</span>
              <input type="number" min="1" class="input input-sm w-20" .value=${String(settings.auto_sync_minutes)} @change=${(e: Event) => this.store.saveSettings({ auto_sync_minutes: Math.max(1, Number((e.target as HTMLInputElement).value) || 5) })} />
              <span class="text-sm">minutes and after each change</span>
            </label>`
          : nothing}
      </div>
    `;
    return html`<omb-modal heading="Device sync" subheading="Peer-to-peer catalog sync over your private network" icon="network" @close=${this.onClosed} .body=${body}></omb-modal>`;
  }
}
