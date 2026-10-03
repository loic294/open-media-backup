import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { FileCategory, FileEntry } from "../../api/types";
import { flowStatus, isRunnable } from "../../state/derived";
import type { DialogRequest } from "../../state/dialogs";
import { flowLabel, spaceFlows } from "../../state/selectors";
import { debounce } from "../../utils/debounce";
import {
  appendExcludeRule,
  exactFilenameExcludePattern,
  extensionExcludePattern,
} from "../../utils/file-rules";
import { formatBytes, formatCount } from "../../utils/format";
import { DialogBase } from "./dialog-base";
import { ref } from "lit/directives/ref.js";
import "../ui/omb-thumbnail";

const PAGE = 120;
const TABS: [FileCategory, string][] = [
  ["to_transfer", "To transfer"],
  ["transferred", "Transferred"],
  ["ignored", "Ignored"],
  ["error", "Failed"],
];

// Top-layer popover escapes the scaled, scrolling modal-box so the menu sits at the cursor unclipped.
function showMenuPopover(el?: Element) {
  if (!(el instanceof HTMLElement)) return;
  // Lit invokes refs before a new fragment is inserted; wait until it is in the document.
  queueMicrotask(() => {
    if (el.isConnected && !el.matches(":popover-open")) el.showPopover?.();
  });
}

/** Thumbnails of what a flow will copy next run, what it already copied, and what its rules skip. */
@customElement("omb-preview-dialog")
export class OmbPreviewDialog extends DialogBase<Extract<DialogRequest, { type: "preview" }>> {
  @state() private flowId: string | null = null;
  @state() private category: FileCategory = "to_transfer";
  @state() private filter = "";
  @state() private view: "grid" | "list" = "grid";
  @state() private items: FileEntry[] = [];
  @state() private menu: { x: number; y: number; file: FileEntry } | null = null;
  @state() private total = 0;
  @state() private totalBytes = 0;
  @state() private loading = false;
  #seq = 0;

  override connectedCallback(): void {
    super.connectedCallback();
    const { snapshot, space, status } = this.store;
    const flows = snapshot && space ? spaceFlows(snapshot, space.id) : [];
    const runnable = flows.find((f) => {
      const fs = flowStatus(status, f.id);
      return fs && isRunnable(fs);
    });
    this.flowId = this.request.flowId ?? runnable?.id ?? flows[0]?.id ?? null;
    this.category = this.request.category ?? "to_transfer";
    window.addEventListener("pointerdown", this.#onPointerDown, true);
    window.addEventListener("keydown", this.#onKeyDown);
    window.addEventListener("scroll", this.#onScroll, true);
    void this.#load(true);
  }

  override disconnectedCallback(): void {
    window.removeEventListener("pointerdown", this.#onPointerDown, true);
    window.removeEventListener("keydown", this.#onKeyDown);
    window.removeEventListener("scroll", this.#onScroll, true);
    super.disconnectedCallback();
  }

  async #load(reset: boolean) {
    const project = this.store.project;
    if (!project || !this.flowId) return;
    const seq = ++this.#seq;
    this.loading = true;
    try {
      const page = await this.store.backend.listFiles({
        projectId: project.id,
        flowId: this.flowId,
        category: this.category,
        offset: reset ? 0 : this.items.length,
        limit: PAGE,
        filter: this.filter || undefined,
      });
      if (seq !== this.#seq) return;
      this.items = reset ? page.items : [...this.items, ...page.items];
      this.total = page.total;
      this.totalBytes = page.total_bytes;
    } catch (e) {
      this.store.toast("error", String(e));
    } finally {
      if (seq === this.#seq) this.loading = false;
    }
  }

  #reload = debounce(() => void this.#load(true), 200);

  #set(patch: { flowId?: string; category?: FileCategory; filter?: string }) {
    Object.assign(this, patch);
    this.menu = null;
    this.items = [];
    if ("filter" in patch) this.#reload();
    else void this.#load(true);
  }

  #openMenu(e: MouseEvent, file: FileEntry) {
    e.preventDefault();
    this.menu = { x: e.clientX, y: e.clientY, file };
  }

  #onPointerDown = (e: PointerEvent) => {
    const target = e.target;
    if (this.menu && (!(target instanceof Element) || !target.closest("[data-preview-context-menu]")))
      this.menu = null;
  };

  #onKeyDown = (e: KeyboardEvent) => {
    if (this.menu && e.key === "Escape") this.menu = null;
  };

  #onScroll = () => {
    if (this.menu) this.menu = null;
  };

  async #exclude(pattern: string) {
    const { snapshot } = this.store;
    const flow = snapshot?.flows.find((f) => f.id === this.flowId);
    const destination = snapshot?.destinations.find((d) => d.id === flow?.destination_id);
    this.menu = null;
    if (!destination) return;
    const update = appendExcludeRule(destination.rules, pattern);
    if (!update.added) {
      this.store.toast("info", `Destination already excludes ${pattern}`);
      return;
    }
    const saved = await this.store.save("destination", { ...destination, rules: update.rules });
    if (saved) {
      this.store.toast("success", `Added ${pattern} to destination exclusions`);
      await this.#load(true);
    }
  }

  #contextMenu() {
    const menu = this.menu;
    if (!menu) return nothing;
    const exact = exactFilenameExcludePattern(menu.file.name);
    const extension = extensionExcludePattern(menu.file.name);
    return html`
      <ul
        data-preview-context-menu
        popover="manual"
        ${ref(showMenuPopover)}
        class="menu menu-sm m-0 w-60 rounded-box bg-base-100 p-2 shadow-lg ring-1 ring-base-300"
        style=${`position:fixed;inset:auto;left:${Math.min(menu.x, window.innerWidth - 248)}px;top:${Math.min(menu.y, window.innerHeight - 96)}px`}
      >
        <li>
          <button type="button" @click=${() => this.#exclude(exact)}>
            Exclude exact filename <span class="font-mono text-xs opacity-70">${menu.file.name}</span>
          </button>
        </li>
        ${
          extension
            ? html`<li>
                <button type="button" @click=${() => this.#exclude(extension)}>
                  Exclude extension <span class="font-mono text-xs opacity-70">${extension}</span>
                </button>
              </li>`
            : nothing
        }
      </ul>
    `;
  }

  #grid() {
    if (this.view === "list") {
      return html`<table class="table table-xs">
        <thead>
          <tr>
            <th>File</th>
            <th>Size</th>
            <th>${this.category === "error" ? "Problem" : "Destination"}</th>
          </tr>
        </thead>
        <tbody>
          ${this.items.map(
            (f) =>
              html`<tr @contextmenu=${(e: MouseEvent) => this.#openMenu(e, f)}>
                <td class="font-mono">${f.rel_path}</td>
                <td class="whitespace-nowrap">${formatBytes(f.size)}</td>
                <td class="font-mono text-base-content/60 ${f.error ? "text-error" : ""}">
                  ${f.error ?? f.target_path ?? "—"}
                </td>
              </tr>`,
          )}
        </tbody>
      </table>`;
    }
    return html`<div class="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-3">
      ${this.items.map((f) => html`<div @contextmenu=${(e: MouseEvent) => this.#openMenu(e, f)}><omb-thumbnail data-omb-block .file=${f}></omb-thumbnail></div>`)}
    </div>`;
  }

  override render() {
    const { snapshot, space } = this.store;
    if (!snapshot || !space) return nothing;
    const flows = spaceFlows(snapshot, space.id);
    const fs = this.flowId ? flowStatus(this.store.status, this.flowId) : undefined;
    const selectedFlow = flows.find((f) => f.id === this.flowId);
    const selectedDestination = snapshot.destinations.find((d) => d.id === selectedFlow?.destination_id);
    const isApp = (selectedDestination?.kind ?? "folder") === "app";
    const counts: Record<FileCategory, number> = {
      to_transfer: fs?.to_transfer ?? 0,
      transferred: fs?.transferred ?? 0,
      ignored: fs?.ignored ?? 0,
      error: fs?.failed ?? 0,
    };
    const body = html`
      <div class="flex flex-col gap-4">
        <div class="flex flex-wrap items-center gap-3">
          <select
            class="select select-sm w-72"
            @change=${(e: Event) => this.#set({ flowId: (e.target as HTMLSelectElement).value })}
          >
            ${flows.map((f) => html`<option value=${f.id} ?selected=${f.id === this.flowId}>${flowLabel(snapshot, f)}</option>`)}
          </select>
          <div role="tablist" class="tabs tabs-box tabs-sm">
            ${TABS.map(
              ([cat, label]) =>
                html`<button
                  role="tab"
                  class="tab gap-1.5 ${this.category === cat ? "tab-active" : ""}"
                  @click=${() => this.#set({ category: cat })}
                >
                  ${label}<span
                    class="badge badge-xs ${cat === "error" && counts.error ? "badge-error" : "badge-ghost"}"
                    >${formatCount(counts[cat])}</span
                  >
                </button>`,
            )}
          </div>
          <span class="flex-1"></span>
          <label class="input input-sm w-56">
            <omb-icon name="search" class="opacity-50"></omb-icon>
            <input
              type="search"
              placeholder="Filter files"
              .value=${this.filter}
              @input=${(e: Event) => this.#set({ filter: (e.target as HTMLInputElement).value })}
            />
          </label>
          <div class="join">
            <button
              class="btn btn-sm btn-square join-item ${this.view === "grid" ? "btn-active" : ""}"
              title="Thumbnails"
              @click=${() => (this.view = "grid")}
            >
              <omb-icon name="layout-grid"></omb-icon>
            </button>
            <button
              class="btn btn-sm btn-square join-item ${this.view === "list" ? "btn-active" : ""}"
              title="List"
              @click=${() => (this.view = "list")}
            >
              <omb-icon name="list"></omb-icon>
            </button>
          </div>
        </div>
        <p class="text-sm text-base-content/60">
          ${formatCount(this.total)} files · ${formatBytes(this.totalBytes)}
        </p>
        ${!flows.length ? html`<p class="text-center py-10 text-base-content/60">Connect a source to a destination to preview files.</p>` : nothing}
        ${this.items.length === 0 && !this.loading && flows.length ? html`<p class="text-center py-10 text-base-content/60">No files here.</p>` : this.#grid()}
        ${this.loading ? html`<div class="grid place-items-center py-4"><span class="loading loading-spinner"></span></div>` : nothing}
        ${
          !this.loading && this.items.length < this.total
            ? html`<button class="btn btn-sm self-center" @click=${() => this.#load(false)}>
                Load ${Math.min(PAGE, this.total - this.items.length)} more
              </button>`
            : nothing
        }
        ${this.#contextMenu()}
      </div>
    `;
    const canRun = fs ? isRunnable(fs) : false;
    const actions = html`
      <button class="btn btn-ghost" @click=${() => this.dismiss()}>Close</button>
      <button
        class="btn"
        ?disabled=${!canRun}
        @click=${() => {
          if (this.flowId) {
            if (isApp) void this.store.openFlowInApp(this.flowId);
            else void this.store.runFlow(this.flowId);
          }
          this.dismiss();
        }}
      >
        <omb-icon name=${isApp ? "external-link" : "play"}></omb-icon
        >${isApp ? "Open in app" : "Run this flow"}
      </button>
      <button class="btn btn-primary" @click=${() => (this.store.runAll(), this.dismiss())}>
        <omb-icon name="play"></omb-icon>Run all transfers
      </button>
    `;
    return html`<omb-modal
      size="xl"
      heading="Preview"
      subheading=${this.store.project?.name ?? ""}
      icon="images"
      @close=${this.onClosed}
      .body=${body}
      .actions=${actions}
    ></omb-modal>`;
  }
}
