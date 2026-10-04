import { html, nothing, type TemplateResult } from "lit";
import { repeat } from "lit/directives/repeat.js";
import { customElement, state } from "lit/decorators.js";
import type { DirectorySummary, FileCategory, FileDirectory, FileEntry } from "../../api/types";
import { flowStatus, isRunnable, projectTotals } from "../../state/derived";
import type { DialogRequest } from "../../state/dialogs";
import { flowLabel, spaceFlows } from "../../state/selectors";
import { debounce } from "../../utils/debounce";
import {
  appendExcludeRule,
  exactFilenameExcludePattern,
  extensionExcludePattern,
} from "../../utils/file-rules";
import { formatBytes, formatCount } from "../../utils/format";
import { childDirectories, directoryKey } from "../../utils/transfer-tree";
import { DialogBase } from "./dialog-base";
import { ref } from "lit/directives/ref.js";
import "../ui/omb-thumbnail";

const PAGE = 120;
interface DirectoryPage {
  items: FileEntry[];
  total: number;
  loading: boolean;
  error: string | null;
}
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
  @state() private view: "tree" | "grid" | "list" = "tree";
  @state() private directories: DirectorySummary[] = [];
  @state() private directoryPages = new Map<string, DirectoryPage>();
  @state() private expanded = new Set<string>();
  @state() private items: FileEntry[] = [];
  @state() private menu: { x: number; y: number; file: FileEntry } | null = null;
  @state() private total = 0;
  @state() private totalBytes = 0;
  @state() private loading = false;
  @state() private error: string | null = null;
  #seq = 0;
  #contextKey = "";

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
    this.#contextKey = this.#workspaceSignature();
    this.store.addEventListener("change", this.#onStoreChange);
    window.addEventListener("pointerdown", this.#onPointerDown, true);
    window.addEventListener("keydown", this.#onKeyDown);
    window.addEventListener("scroll", this.#onScroll, true);
    void this.#load(true);
  }

  override disconnectedCallback(): void {
    this.#seq++;
    this.store.removeEventListener("change", this.#onStoreChange);
    window.removeEventListener("pointerdown", this.#onPointerDown, true);
    window.removeEventListener("keydown", this.#onKeyDown);
    window.removeEventListener("scroll", this.#onScroll, true);
    super.disconnectedCallback();
  }

  async #load(reset: boolean) {
    const context = this.store.context;
    if (!context || !this.flowId) return;
    const seq = ++this.#seq;
    if (reset) {
      this.directories = [];
      this.directoryPages = new Map();
      this.expanded = new Set();
    }
    this.loading = true;
    this.error = null;
    try {
      const page = await this.store.listWorkspaceFiles({
        context,
        flowId: this.flowId,
        category: this.category,
        offset: reset ? 0 : this.items.length,
        limit: PAGE,
        filter: this.filter || undefined,
        directory: this.view === "tree" ? this.#rootDirectory() : undefined,
      });
      if (seq !== this.#seq) return;
      this.items = reset ? page.items : [...this.items, ...page.items];
      if (this.view === "tree") {
        if (!page.directories) throw new Error("Folder summaries were not returned by the backend");
        this.directories = page.directories;
        const roots = page.directories.filter((directory) => directory.path === "");
        this.total = roots.reduce((sum, root) => sum + root.total, 0);
        this.totalBytes = roots.reduce((sum, root) => sum + root.total_bytes, 0);
        const root = this.#rootDirectory();
        this.directoryPages = new Map([
          [
            directoryKey(root),
            {
              items: page.items,
              total: page.total,
              loading: false,
              error: null,
            },
          ],
        ]);
        this.expanded = new Set(roots.map(directoryKey));
      } else {
        this.total = page.total;
        this.totalBytes = page.total_bytes;
      }
    } catch (e) {
      if (seq !== this.#seq) return;
      this.error = String(e);
      this.store.toast("error", String(e));
    } finally {
      if (seq === this.#seq) this.loading = false;
    }
  }

  #reload = debounce(() => void this.#load(true), 200);

  #isApp(): boolean {
    const snapshot = this.store.snapshot;
    const flow = snapshot?.flows.find((item) => item.id === this.flowId);
    return snapshot?.destinations.find((item) => item.id === flow?.destination_id)?.kind === "app";
  }

  #rootDirectory(): FileDirectory {
    return { kind: this.#isApp() || this.category === "ignored" ? "source" : "destination", path: "" };
  }

  #changeView(view: "tree" | "grid" | "list") {
    if (view === this.view) return;
    this.view = view;
    this.#set({});
  }

  async #loadDirectory(directory: FileDirectory) {
    const context = this.store.context;
    if (!context || !this.flowId) return;
    const seq = this.#seq;
    const key = directoryKey(directory);
    const previous = this.directoryPages.get(key);
    if (previous?.loading) return;
    const update = (page: DirectoryPage) => {
      this.directoryPages = new Map(this.directoryPages).set(key, page);
    };
    update({ items: previous?.items ?? [], total: previous?.total ?? 0, loading: true, error: null });
    try {
      const page = await this.store.listWorkspaceFiles({
        context,
        flowId: this.flowId,
        category: this.category,
        filter: this.filter || undefined,
        directory,
        offset: previous?.items.length ?? 0,
        limit: PAGE,
      });
      if (seq !== this.#seq) return;
      update({
        items: [...(previous?.items ?? []), ...page.items],
        total: page.total,
        loading: false,
        error: null,
      });
    } catch (error) {
      if (seq !== this.#seq) return;
      update({
        items: previous?.items ?? [],
        total: previous?.total ?? 0,
        loading: false,
        error: String(error),
      });
      this.store.toast("error", String(error));
    }
  }

  #toggleDirectory(directory: DirectorySummary) {
    const key = directoryKey(directory);
    const expanded = new Set(this.expanded);
    if (expanded.has(key)) expanded.delete(key);
    else {
      expanded.add(key);
      if (directory.direct_files && !this.directoryPages.has(key)) void this.#loadDirectory(directory);
    }
    this.expanded = expanded;
  }

  #workspaceSignature(): string {
    const snapshot = this.store.snapshot;
    return JSON.stringify({
      context: this.store.context,
      space: this.store.space,
      projects: snapshot?.projects,
      sources: snapshot?.sources,
      destinations: snapshot?.destinations,
      devices: snapshot?.devices,
      flows: snapshot?.flows,
      mappings: snapshot?.mappings,
    });
  }

  #onStoreChange = () => {
    const key = this.#workspaceSignature();
    if (key === this.#contextKey) return;
    this.#contextKey = key;
    const flows =
      this.store.snapshot && this.store.space ? spaceFlows(this.store.snapshot, this.store.space.id) : [];
    this.flowId = flows.find((f) => f.id === this.flowId)?.id ?? flows[0]?.id ?? null;
    this.#set({});
  };

  #set(patch: { flowId?: string; category?: FileCategory; filter?: string }) {
    this.#seq++;
    this.loading = false;
    Object.assign(this, patch);
    this.menu = null;
    this.items = [];
    this.total = 0;
    this.totalBytes = 0;
    this.directories = [];
    this.directoryPages = new Map();
    this.expanded = new Set();
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
    if (this.view === "tree") return this.#tree();
    if (this.view === "list") {
      return html`<table class="table table-xs">
        <thead>
          <tr>
            <th>File</th>
            <th>Size</th>
            <th>
              ${this.category === "error" ? "Problem" : this.category === "ignored" ? "Reason" : "Destination"}
            </th>
          </tr>
        </thead>
        <tbody>
          ${this.items.map(
            (f) =>
              html`<tr @contextmenu=${(e: MouseEvent) => this.#openMenu(e, f)}>
                <td class="font-mono">${f.rel_path}</td>
                <td class="whitespace-nowrap">${formatBytes(f.size)}</td>
                <td class="text-base-content/60 ${f.error ? "text-error" : ""}">
                  ${this.category === "ignored" ? (f.ignore_reason ?? "Reason unavailable") : (f.error ?? f.target_path ?? "—")}
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

  #treeDirectory(directory: DirectorySummary): TemplateResult {
    const key = directoryKey(directory);
    const page = this.directoryPages.get(key);
    const children = childDirectories(this.directories, directory);
    const open = this.expanded.has(key);
    const label = directory.path
      ? directory.path.split("/").at(-1)!
      : directory.kind === "destination"
        ? "Destination root (planned paths)"
        : this.#isApp()
          ? "Source folders (open in app)"
          : "No destination (source paths)";
    return html`<li>
      <details .open=${open}>
        <summary
          title=${directory.path || label}
          @click=${(event: MouseEvent) => {
            event.preventDefault();
            this.#toggleDirectory(directory);
          }}
        >
          <omb-icon name="folder" class="text-primary"></omb-icon>
          <span class="break-all font-mono">${label}</span>
          <span class="text-xs text-base-content/60"
            >${formatCount(directory.total)} files · ${formatBytes(directory.total_bytes)}</span
          >
        </summary>
        ${
          open
            ? html`<ul>
                ${repeat(children, directoryKey, (child) => this.#treeDirectory(child))}
                ${repeat(
                  page?.items ?? [],
                  (file) => JSON.stringify([file.rel_path, file.project_id, file.target_path]),
                  (file) =>
                    html`<li>
                      <div
                        @contextmenu=${(event: MouseEvent) => this.#openMenu(event, file)}
                        title=${`Source: ${file.rel_path}${file.target_path && !this.#isApp() ? `\nPlanned destination: ${file.target_path}` : ""}`}
                      >
                        <omb-icon name="file"></omb-icon>
                        <span class="break-all font-mono">${file.name}</span>
                        <span class="text-xs text-base-content/60">${formatBytes(file.size)}</span>
                        ${file.error ? html`<span class="text-xs text-error">${file.error}</span>` : nothing}
                      </div>
                    </li>`,
                )}
                ${
                  page?.loading
                    ? html`<li>
                        <span role="status"
                          ><span class="loading loading-spinner loading-xs"></span>Loading files</span
                        >
                      </li>`
                    : nothing
                }
                ${page?.error ? html`<li><span class="text-error">${page.error}</span><button @click=${() => this.#loadDirectory(directory)}>Retry</button></li>` : nothing}
                ${directory.direct_files && !page ? html`<li><button @click=${() => this.#loadDirectory(directory)}>Load files</button></li>` : nothing}
                ${page && !page.loading && !page.error && page.items.length < page.total ? html`<li><button @click=${() => this.#loadDirectory(directory)}>Load ${Math.min(PAGE, page.total - page.items.length)} more files</button></li>` : nothing}
              </ul>`
            : nothing
        }
      </details>
    </li>`;
  }

  #tree() {
    return html`<ul
      class="menu menu-sm w-full rounded-box border border-base-300"
      aria-label="Transfer folder structure"
    >
      ${repeat(
        this.directories.filter((directory) => directory.path === ""),
        directoryKey,
        (root) => this.#treeDirectory(root),
      )}
    </ul>`;
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
              class="btn btn-sm btn-square join-item ${this.view === "tree" ? "btn-active" : ""}"
              title="Folder structure"
              aria-label="Folder structure"
              aria-pressed=${this.view === "tree"}
              @click=${() => this.#changeView("tree")}
            >
              <omb-icon name="folder"></omb-icon>
            </button>
            <button
              class="btn btn-sm btn-square join-item ${this.view === "grid" ? "btn-active" : ""}"
              title="Thumbnails"
              @click=${() => this.#changeView("grid")}
            >
              <omb-icon name="layout-grid"></omb-icon>
            </button>
            <button
              class="btn btn-sm btn-square join-item ${this.view === "list" ? "btn-active" : ""}"
              title="List"
              @click=${() => this.#changeView("list")}
            >
              <omb-icon name="list"></omb-icon>
            </button>
          </div>
        </div>
        <p class="text-sm text-base-content/60">
          ${formatCount(this.total)} files · ${formatBytes(this.totalBytes)}
        </p>
        ${
          this.error
            ? html`<div role="alert" class="alert alert-error alert-soft sm:alert-horizontal">
                <span>Could not load files: ${this.error}</span>
                <button class="btn btn-sm" @click=${() => this.#load(true)}>Retry</button>
              </div>`
            : nothing
        }
        ${!flows.length ? html`<p class="text-center py-10 text-base-content/60">Connect a source to a destination to preview files.</p>` : nothing}
        ${this.total === 0 && !this.loading && !this.error && flows.length ? html`<p class="text-center py-10 text-base-content/60">No files here.</p>` : this.#grid()}
        ${this.loading ? html`<div class="grid place-items-center py-4"><span class="loading loading-spinner"></span></div>` : nothing}
        ${
          this.view !== "tree" && !this.loading && this.items.length < this.total
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
      <button
        class="btn btn-primary"
        ?disabled=${!projectTotals(this.store.status, flows, snapshot.destinations).runnable}
        @click=${() => (this.store.runAll(), this.dismiss())}
      >
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
