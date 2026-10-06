import { html, nothing, type PropertyValues } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import type { HashRoot, RemoteHash } from "../../api/types";
import { OmbElement } from "../ui/omb-element";

@customElement("omb-remote-hash-field")
export class OmbRemoteHashField extends OmbElement {
  @property({ attribute: false }) mapping: RemoteHash | null = null;
  @property() destinationId = "";
  @property() deviceId = "";
  @state() private roots: HashRoot[] = [];
  @state() private directories: string[] = [];
  @state() private error = "";
  @state() private busy = false;
  #server = "";
  #known = false;
  #seq = 0;

  protected override willUpdate(changed: PropertyValues): void {
    super.willUpdate(changed);
    const server = this.mapping?.server_id ?? "";
    const known = this.store.hashServers.some((s) => s.id === server);
    if (server !== this.#server || known !== this.#known) {
      this.#server = server;
      this.#known = known;
      this.roots = [];
      this.directories = [];
      this.error = "";
      void this.#loadRoots();
    }
  }

  #set(patch: Partial<RemoteHash>) {
    this.dispatchEvent(
      new CustomEvent<RemoteHash>("remote-hash-change", {
        detail: { server_id: "", root: "", enabled: false, ...this.mapping, ...patch },
        bubbles: true,
        composed: true,
      }),
    );
  }

  async #loadRoots() {
    const server = this.#server;
    const seq = ++this.#seq;
    if (!server || !this.store.hashServers.some((s) => s.id === server)) {
      this.busy = false;
      return;
    }
    this.busy = true;
    try {
      const roots = await this.store.backend.hashServerRoots(server);
      if (seq !== this.#seq) return;
      this.roots = roots;
      this.error = "";
    } catch (error) {
      if (seq === this.#seq) this.error = String(error);
    } finally {
      if (seq === this.#seq) this.busy = false;
    }
  }

  async #browse() {
    const mapping = this.mapping;
    if (!mapping?.root) return;
    const seq = ++this.#seq;
    this.busy = true;
    try {
      const result = await this.store.backend.hashServerBrowse(mapping.server_id, mapping.root, "");
      if (seq !== this.#seq) return;
      this.directories = result.directories;
      this.error = "";
    } catch (error) {
      if (seq === this.#seq) this.error = String(error);
    } finally {
      if (seq === this.#seq) this.busy = false;
    }
  }

  async #test() {
    this.busy = true;
    try {
      const result = await this.store.backend.testRemoteHashMapping(this.destinationId);
      this.store.toast(result.verified ? "success" : "warning", result.message, 10000);
    } catch (error) {
      this.store.toast("error", String(error));
    } finally {
      this.busy = false;
    }
  }

  override render() {
    const m = this.mapping;
    const servers = this.store.hashServers;
    const unknown = !!m?.server_id && !servers.some((s) => s.id === m.server_id);
    const [rootId = "", ...subfolders] = (m?.root ?? "").split("/");
    const selectedRoot = this.roots.find((r) => r.id === rootId);
    const saved = this.store.snapshot?.destinations.find((d) => d.id === this.destinationId);
    const canTest =
      !!m?.root &&
      !unknown &&
      saved?.device_id === this.deviceId &&
      saved.remote_hash?.server_id === m.server_id &&
      saved.remote_hash?.root === m.root;
    return html`<section
      aria-label="Remote hash check"
      class="flex flex-col gap-3 rounded-box border border-base-300 p-4"
    >
      <label class="flex items-start gap-3 cursor-pointer">
        <input
          aria-label="Remote hash check"
          type="checkbox"
          class="toggle mt-0.5"
          .checked=${m?.enabled ?? false}
          @change=${(e: Event) => this.#set({ enabled: (e.target as HTMLInputElement).checked })}
        />
        <span
          ><span class="font-medium">Remote hash check</span
          ><span class="block text-sm text-base-content/60">
            Re-read verification, existing-file comparisons and Check destination run on the NAS. Inline copy
            verification stays unchanged.
          </span></span
        >
      </label>
      ${
        m?.enabled
          ? html`
              <p class="text-sm text-base-content/60">
                Choose the remote folder that matches the destination <strong>device root</strong>, not its
                project or destination subfolder. Failures fall back to a local re-read with a job warning and
                toast.
              </p>
              <label class="flex flex-col gap-1 text-sm"
                >Hash server
                <select
                  aria-label="Hash server"
                  class="select w-full"
                  ?disabled=${this.busy}
                  @change=${(e: Event) => this.#set({ server_id: (e.target as HTMLSelectElement).value, root: "" })}
                >
                  <option value="" .selected=${!m.server_id}>Select a server</option>
                  ${unknown ? html`<option .value=${m.server_id} selected>${m.server_id} — not available on this computer</option>` : nothing}
                  ${servers.map((s) => html`<option .value=${s.id} .selected=${s.id === m.server_id}>${s.name} (${s.address})</option>`)}
                </select>
              </label>
              ${unknown ? html`<p class="text-sm text-warning">Hash server not available on this computer. Add it in Device sync to use this synced mapping. Checks here use local re-reads.</p>` : nothing}
              ${!servers.length && !unknown ? html`<p class="text-sm text-base-content/60">Add a hash server in Device sync first.</p>` : nothing}
              ${
                m.server_id && !unknown
                  ? html`
                      <label class="flex flex-col gap-1 text-sm"
                        >Remote device-root folder
                        <select
                          aria-label="Remote device-root folder"
                          class="select w-full"
                          ?disabled=${this.busy}
                          @change=${(e: Event) => {
                            this.directories = [];
                            this.#set({ root: (e.target as HTMLSelectElement).value });
                          }}
                        >
                          <option value="" .selected=${!rootId}>Select an exposed folder</option>
                          ${rootId && !selectedRoot ? html`<option .value=${rootId} selected>Saved folder (${rootId.slice(0, 12)}…)</option>` : nothing}
                          ${this.roots.map((r) => html`<option .value=${r.id} .selected=${r.id === rootId}>${r.path}</option>`)}
                        </select>
                      </label>
                      ${
                        m.root
                          ? html`<div class="flex flex-col gap-2">
                              <p class="text-sm font-mono break-all">
                                ${selectedRoot?.path ?? rootId}${subfolders.length ? `/${subfolders.join("/")}` : ""}
                              </p>
                              <div class="flex flex-wrap gap-2">
                                <button
                                  type="button"
                                  class="btn btn-sm"
                                  ?disabled=${this.busy}
                                  @click=${() => this.#browse()}
                                >
                                  Browse subfolders
                                </button>
                                ${
                                  subfolders.length
                                    ? html`<button
                                        type="button"
                                        class="btn btn-sm btn-ghost"
                                        ?disabled=${this.busy}
                                        @click=${() => {
                                          this.directories = [];
                                          this.#set({ root: [rootId, ...subfolders.slice(0, -1)].join("/") });
                                        }}
                                      >
                                        Parent folder
                                      </button>`
                                    : nothing
                                }
                                ${this.directories.map(
                                  (name) =>
                                    html`<button
                                      type="button"
                                      class="btn btn-sm btn-ghost"
                                      ?disabled=${this.busy}
                                      @click=${() => {
                                        this.directories = [];
                                        this.#set({ root: `${m.root}/${name}` });
                                      }}
                                    >
                                      <omb-icon name="folder"></omb-icon>${name}
                                    </button>`,
                                )}
                              </div>
                            </div>`
                          : nothing
                      }
                      ${
                        this.error
                          ? html`<p role="alert" class="text-sm text-error">${this.error}</p>
                              <button class="btn btn-sm self-start" @click=${() => this.#loadRoots()}>
                                Retry folders
                              </button>`
                          : nothing
                      }
                      <button
                        type="button"
                        class="btn btn-sm self-start"
                        ?disabled=${this.busy || !canTest}
                        @click=${() => this.#test()}
                      >
                        Test mapping
                      </button>
                      ${!canTest ? html`<p class="text-xs text-base-content/60">Save the destination and reopen it to test a changed mapping.</p>` : nothing}
                    `
                  : nothing
              }
            `
          : nothing
      }
    </section>`;
  }
}
