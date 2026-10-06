import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import { formatAgo } from "../../utils/format";
import { OmbElement } from "../ui/omb-element";

@customElement("omb-hash-servers-section")
export class OmbHashServersSection extends OmbElement {
  @state() private address = "";
  @state() private token = "";
  @state() private busy = false;

  override connectedCallback(): void {
    super.connectedCallback();
    void this.store.refreshHashServers();
  }

  async #run(action: () => Promise<void>, message: string) {
    this.busy = true;
    try {
      await action();
      await this.store.refreshHashServers();
      this.store.toast("success", message);
    } catch (error) {
      this.store.toast("error", String(error));
      await this.store.refreshHashServers();
    } finally {
      this.busy = false;
    }
  }

  override render() {
    return html`<section aria-label="Hash servers" class="flex flex-col gap-3">
      <h4 class="font-medium">Hash servers</h4>
      <p class="text-sm text-base-content/60">
        Verify destination files locally on your NAS instead of reading every byte back over the network.
        Pairing credentials stay on this computer. Use plain HTTP only on a trusted LAN or Netbird.
      </p>
      ${
        this.store.hashServers.length
          ? html`<ul class="divide-y divide-base-300">
              ${this.store.hashServers.map(
                (server) =>
                  html`<li class="flex items-start gap-3 py-3">
                    <div class="min-w-0 flex-1">
                      <div class="font-medium">
                        ${server.name}
                        <span class="badge badge-sm ${server.last_error ? "badge-warning" : "badge-ghost"}">
                          ${server.last_error ? "Last check failed" : "Paired"}
                        </span>
                      </div>
                      <p class="text-xs text-base-content/60">
                        ${server.address} · last seen ${formatAgo(server.last_seen)}
                      </p>
                      ${server.last_error ? html`<p class="text-xs text-warning">${server.last_error}</p>` : nothing}
                    </div>
                    <button
                      class="btn btn-sm btn-ghost"
                      ?disabled=${this.busy}
                      @click=${() =>
                        this.#run(async () => {
                          await this.store.backend.hashServerRoots(server.id);
                        }, "Hash server reachable")}
                    >
                      Test
                    </button>
                    <button
                      class="btn btn-ghost btn-sm btn-square"
                      title="Remove hash server"
                      ?disabled=${this.busy}
                      @click=${() =>
                        this.store.open({
                          type: "confirm",
                          title: `Remove ${server.name}?`,
                          message:
                            "Credentials are removed from this computer only. Synced destination mappings remain; checks will fall back to local re-reads.",
                          confirmLabel: "Remove",
                          danger: true,
                          onConfirm: () =>
                            this.#run(
                              () => this.store.backend.removeHashServer(server.id),
                              "Hash server removed",
                            ),
                        })}
                    >
                      <omb-icon name="trash"></omb-icon>
                    </button>
                  </li>`,
              )}
            </ul>`
          : html`<p class="text-sm text-base-content/60">No hash servers added on this computer.</p>`
      }
      <div class="flex flex-col sm:flex-row gap-2">
        <input
          aria-label="Hash server address"
          class="input input-sm flex-1 font-mono"
          placeholder="100.1.2.3:47822"
          .value=${this.address}
          @input=${(e: Event) => (this.address = (e.target as HTMLInputElement).value.trim())}
        />
        <input
          aria-label="Hash server token"
          type="password"
          autocomplete="off"
          class="input input-sm flex-1 font-mono"
          placeholder="Pairing token"
          .value=${this.token}
          @input=${(e: Event) => (this.token = (e.target as HTMLInputElement).value.trim())}
        />
        <button
          class="btn btn-sm"
          ?disabled=${this.busy || !this.address || !this.token}
          @click=${() =>
            this.#run(async () => {
              await this.store.backend.addHashServer(this.address, this.token);
              this.address = this.token = "";
            }, "Hash server added")}
        >
          Add hash server
        </button>
      </div>
    </section>`;
  }
}
