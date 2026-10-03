import { html, nothing } from "lit";
import { repeat } from "lit/directives/repeat.js";
import { customElement, state } from "lit/decorators.js";
import type { FileCategory } from "../../api/types";
import type { Project, Snapshot, Source } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { selectMedia, type MediaSelection } from "../../state/media-selection";
import { matchingProjects } from "../../state/projects";
import { deviceById } from "../../state/selectors";
import { debounce } from "../../utils/debounce";
import { openInAppLabel } from "../../utils/preview-apps";
import { DialogBase } from "./dialog-base";
import {
  captureLabel,
  captureTime,
  captureTimeMs,
  browserDirectory,
  mergeMedia,
  selectedCaptureRange,
  type BrowserMedia,
} from "./media-browser-data";
import "./media-inspector";
import "../ui/omb-thumbnail";

const PAGE = 60;
const REQUESTS_PER_LOAD = 4;
const CATEGORIES: FileCategory[] = ["to_transfer", "transferred", "ignored", "error"];
const VALID_PROJECT_COLOR = /^#[0-9a-f]{6}$/i;
interface Cursor {
  flowId: string;
  category: FileCategory;
  offset: number;
  done: boolean;
}

function projectColor(project: Project): string {
  return VALID_PROJECT_COLOR.test(project.color ?? "") ? project.color! : "var(--color-primary)";
}

function projectsForMedia(snapshot: Snapshot, source: Source, item: BrowserMedia): Project[] {
  const matchedIds = new Set(
    matchingProjects(snapshot, source, item.file.capture_time).map((project) => project.id),
  );
  return snapshot.projects.filter(
    (project) =>
      project.space_id === source.space_id &&
      (matchedIds.has(project.id) || item.projectIds.includes(project.id)),
  );
}

@customElement("omb-media-browser-dialog")
export class OmbMediaBrowserDialog extends DialogBase<Extract<DialogRequest, { type: "media-browser" }>> {
  @state() private items: BrowserMedia[] = [];
  @state() private selection: MediaSelection = { selected: new Set(), anchor: null };
  @state() private inspected: string | null = null;
  @state() private loading = false;
  @state() private filter = "";
  @state() private error: string | null = null;
  @state() private selectionError: string | null = null;
  @state() private creatingProject = false;
  @state() private hasMore = false;
  @state() private currentDir = "";
  #cursors: Cursor[] = [];
  #next = 0;
  #seq = 0;
  #projectId: string | null = null;
  #projectsSignature = "";

  override connectedCallback(): void {
    super.connectedCallback();
    this.#projectId = this.store.project?.id ?? null;
    this.#projectsSignature = this.#signature();
    this.store.addEventListener("change", this.#onStoreChange);
    this.#reset();
  }

  override disconnectedCallback(): void {
    this.#seq++;
    this.store.removeEventListener("change", this.#onStoreChange);
    super.disconnectedCallback();
  }

  #signature(): string {
    const snapshot = this.store.snapshot;
    const source = snapshot?.sources.find((item) => item.id === this.request.sourceId);
    return JSON.stringify({
      activeProjectId: this.store.project?.id ?? null,
      sourceScope: source?.project_scope ?? { mode: "all" },
      projects: snapshot?.projects
        .filter((project) => project.space_id === source?.space_id)
        .map((project) => [
          project.id,
          project.start_time ?? null,
          project.end_time ?? null,
          project.granularity ?? "minute",
          project.color ?? null,
          project.archived,
        ]),
    });
  }

  #onStoreChange = () => {
    const projectId = this.store.project?.id ?? null;
    const signature = this.#signature();
    if (projectId !== this.#projectId) {
      this.#projectId = projectId;
      this.#projectsSignature = signature;
      this.#reload();
      return;
    }
    if (signature !== this.#projectsSignature) {
      this.#projectsSignature = signature;
      this.requestUpdate();
    }
  };

  #reset() {
    this.#seq++;
    this.loading = false;
    this.items = [];
    this.selection = { selected: new Set(), anchor: null };
    this.inspected = null;
    this.currentDir = "";
    this.#next = 0;
    const flows = this.store.snapshot?.flows.filter((flow) => flow.source_id === this.request.sourceId) ?? [];
    this.#cursors = CATEGORIES.flatMap((category) =>
      flows.map((flow) => ({ flowId: flow.id, category, offset: 0, done: false })),
    );
    this.hasMore = this.#cursors.length > 0 && this.#projectId !== null;
    void this.#load();
  }

  #reload = debounce(() => this.isConnected && this.#reset(), 200);

  async #load() {
    if (this.loading || !this.hasMore || !this.#projectId) return;
    const seq = ++this.#seq;
    this.loading = true;
    this.error = null;
    const batch: Cursor[] = [];
    for (let visited = 0; visited < this.#cursors.length && batch.length < REQUESTS_PER_LOAD; visited++) {
      const cursor = this.#cursors[this.#next];
      this.#next = (this.#next + 1) % this.#cursors.length;
      if (!cursor.done) batch.push(cursor);
    }
    await Promise.all(
      batch.map(async (cursor) => {
        try {
          const page = await this.store.backend.listFiles({
            projectId: this.#projectId!,
            flowId: cursor.flowId,
            category: cursor.category,
            offset: cursor.offset,
            limit: PAGE,
            filter: this.filter || undefined,
          });
          if (seq !== this.#seq) return;
          cursor.offset += page.items.length;
          cursor.done = cursor.offset >= page.total || page.items.length === 0;
          this.items = mergeMedia(this.items, page.items);
        } catch (error) {
          if (seq !== this.#seq) return;
          this.error = `Could not load media: ${error}`;
          this.store.toast("error", this.error);
        }
      }),
    );
    if (seq !== this.#seq) return;
    this.hasMore = this.#cursors.some((cursor) => !cursor.done);
    this.loading = false;
  }

  #select(key: string, event: Pick<MouseEvent, "shiftKey" | "ctrlKey" | "metaKey">, visibleKeys: string[]) {
    if (this.creatingProject) return;
    this.selectionError = null;
    this.selection = selectMedia(visibleKeys, this.selection, key, {
      extend: event.shiftKey,
      toggle: event.ctrlKey || event.metaKey,
    });
    this.inspected = key;
  }

  #openFolder(path: string) {
    this.currentDir = path;
    this.inspected = null;
  }

  #openParent() {
    const parts = this.currentDir.split("/").filter(Boolean);
    this.#openFolder(parts.slice(0, -1).join("/"));
  }

  async #createProjectFromSelection() {
    if (!this.selection.selected.size || this.creatingProject) return;
    const selected = this.items.filter((item) => this.selection.selected.has(item.file.rel_path));
    if (selected.length !== this.selection.selected.size) {
      this.selectionError = "Some selected files are no longer available. Clear the selection and try again.";
      return;
    }

    this.creatingProject = true;
    this.selectionError = null;
    try {
      const times: (number | null)[] = [];
      for (let start = 0; start < selected.length; start += 4) {
        const batch = await Promise.all(
          selected.slice(start, start + 4).map(async ({ file }) => {
            const listedTime = captureTime(file);
            if (listedTime !== null) return listedTime;
            if (!file.abs_path) return null;
            const metadata = await this.store.backend.getMediaMetadata(file.abs_path);
            return captureTimeMs(metadata.capture_time);
          }),
        );
        times.push(...batch);
      }
      if (times.some((time) => time === null)) {
        this.selectionError =
          "Every selected file needs an embedded capture date and known timezone to create a project. Unknown dates are not inferred.";
        return;
      }
      const validTimes = times.filter((time): time is number => time !== null);
      this.store.open({
        type: "project",
        projectId: null,
        start_time: Math.min(...validTimes),
        end_time: Math.max(...validTimes),
      });
    } catch (error) {
      this.selectionError = `Could not read selected files' embedded capture times: ${error}`;
      this.store.toast("error", this.selectionError);
    } finally {
      this.creatingProject = false;
    }
  }

  async #openInApp(file: BrowserMedia["file"]) {
    if (!file.abs_path || !this.store.snapshot) return;
    const label = openInAppLabel(this.store.snapshot.settings, this.store.snapshot.computer.os, file);
    try {
      await this.store.backend.openMedia(file.abs_path);
      this.store.toast("success", `${label}: ${file.name}`);
    } catch (error) {
      this.store.toast("error", `Could not open ${file.name}: ${error}`);
    }
  }

  override render() {
    const snapshot = this.store.snapshot;
    const source = snapshot?.sources.find((item) => item.id === this.request.sourceId);
    if (!snapshot || !source) return nothing;
    const directory = browserDirectory(
      this.items.map((item) => item.file.rel_path),
      this.currentDir,
    );
    const visibleKeys = directory.files;
    const visible = directory.files
      .map((path) => this.items.find((item) => item.file.rel_path === path))
      .filter((item): item is BrowserMedia => item !== undefined);
    const file = this.items.find((item) => item.file.rel_path === this.inspected)?.file;
    const range = selectedCaptureRange(this.items, this.selection.selected);
    const body = html` <div class="flex flex-col gap-4 lg:flex-1 lg:min-h-0">
      <div class="flex flex-wrap items-center gap-3">
        <label class="input input-sm">
          <omb-icon name="search"></omb-icon>
          <input
            aria-label="Filter source media"
            type="search"
            placeholder="Filter files"
            .value=${this.filter}
            @input=${(event: Event) => {
              this.filter = (event.target as HTMLInputElement).value;
              this.#seq++;
              this.currentDir = "";
              this.#reload();
            }}
          />
        </label>
        <span class="text-sm" aria-live="polite"
          >${this.items.length} unique files loaded · ${this.selection.selected.size} selected</span
        >
        <button
          class="btn btn-sm"
          ?disabled=${!this.selection.selected.size || this.creatingProject}
          @click=${() => {
            this.selection = { selected: new Set(), anchor: null };
            this.selectionError = null;
          }}
        >
          Clear selection
        </button>
      </div>
      <p class="text-xs text-base-content/60">
        Sorted by embedded capture time (UTC), then path; unknown dates last. Shift-click selects an inclusive
        range. Ctrl/Cmd-click toggles selection. Focus a tile and press Enter or Space to select. Only loaded
        files are included; loading more may insert earlier captures.
      </p>
      ${!this.#projectId ? html`<p role="status">Select a project before browsing source media.</p>` : nothing}
      ${!this.#cursors.length ? html`<p role="status">Connect this source to a destination to browse its files.</p>` : nothing}
      ${this.error ? html`<p role="alert" class="text-error">${this.error} Use Load more to retry.</p>` : nothing}
      <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div class="breadcrumbs max-w-full rounded-box border border-base-300 px-3 py-1 text-sm">
          <ul>
            ${directory.breadcrumbs.map(
              (crumb) =>
                html`<li>
                  <button
                    type="button"
                    class="link-hover ${crumb.path === directory.currentDir ? "font-semibold text-primary" : ""}"
                    @click=${() => this.#openFolder(crumb.path)}
                    aria-current=${crumb.path === directory.currentDir ? "page" : nothing}
                  >
                    ${crumb.label}
                  </button>
                </li>`,
            )}
          </ul>
        </div>
        <button
          type="button"
          class="btn btn-sm sm:shrink-0"
          ?disabled=${!directory.currentDir}
          @click=${() => this.#openParent()}
        >
          ↑ Up
        </button>
      </div>
      <div class="flex flex-col lg:flex-row gap-5 lg:flex-1 lg:min-h-0">
        <section
          class="flex-1 min-w-0 lg:min-h-0 lg:overflow-y-auto lg:pr-2"
          aria-label="Source media gallery"
          aria-busy=${this.loading}
        >
          <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3">
            ${repeat(
              directory.folders,
              (folder) => folder.path,
              (folder) =>
                html`<button
                  type="button"
                  class="card card-border bg-base-100 text-left transition hover:bg-base-200 focus-visible:outline-2 focus-visible:outline-primary"
                  aria-label=${`Open folder ${folder.path}`}
                  title="Open folder"
                  @click=${() => this.#openFolder(folder.path)}
                >
                  <div class="card-body gap-3 p-4">
                    <omb-icon name="folder" class="size-7 text-primary"></omb-icon>
                    <div>
                      <h3 class="card-title text-base">${folder.name}</h3>
                      <p class="text-sm text-base-content/60">${folder.itemCount} items</p>
                    </div>
                  </div>
                </button>`,
            )}
            ${repeat(
              visible,
              (item) => item.file.rel_path,
              (item) => {
                const selected = this.selection.selected.has(item.file.rel_path);
                const projects = projectsForMedia(snapshot, source, item);
                const firstProjectColor = projects[0] && !selected ? projectColor(projects[0]) : null;
                return html` <div class="flex flex-col gap-1 min-w-0">
                  <button
                    type="button"
                    class="rounded-box p-2 text-left border border-base-300 focus-visible:outline-2 focus-visible:outline-primary ${selected ? "ring-4 ring-primary ring-offset-2 ring-offset-base-100 bg-primary/10 border-primary" : "hover:bg-base-200"}"
                    style=${firstProjectColor ? `border-color: ${firstProjectColor}` : nothing}
                    aria-label=${`Select ${item.file.rel_path}`}
                    aria-pressed=${selected}
                    title=${
                      item.file.abs_path
                        ? `Double-click to ${openInAppLabel(snapshot.settings, snapshot.computer.os, item.file).replace(/^Open/, "open")}`
                        : "Source file is not available to open"
                    }
                    @click=${(event: MouseEvent) => this.#select(item.file.rel_path, event, visibleKeys)}
                    @dblclick=${() => {
                      this.#select(
                        item.file.rel_path,
                        { shiftKey: false, ctrlKey: false, metaKey: false },
                        visibleKeys,
                      );
                      void this.#openInApp(item.file);
                    }}
                    @keydown=${(event: KeyboardEvent) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        this.#select(item.file.rel_path, event, visibleKeys);
                      }
                    }}
                  >
                    <omb-thumbnail .file=${item.file}></omb-thumbnail>
                    <span class="block text-xs text-base-content/60 mt-1">${captureLabel(item.file)}</span>
                  </button>
                  <div class="flex flex-wrap gap-1">
                    ${projects.map(
                      (project) =>
                        html`<span class="badge badge-sm badge-outline max-w-full" title=${project.name}>
                          <span
                            class="size-2 rounded-full shrink-0"
                            style=${`background-color: ${projectColor(project)}`}
                          ></span>
                          <span class="truncate">${project.name}</span>
                        </span>`,
                    )}
                  </div>
                </div>`;
              },
            )}
          </div>
          ${
            !directory.folders.length &&
            !visible.length &&
            !this.loading &&
            !this.error &&
            this.#cursors.length &&
            this.#projectId
              ? html`<p role="status" class="py-6 text-base-content/60">
                  ${this.hasMore ? "No files in these pages. Load more to check remaining routes." : "No files found."}
                </p>`
              : nothing
          }
          ${this.loading ? html`<p role="status" class="py-4"><span class="loading loading-spinner loading-sm"></span> Loading media…</p>` : nothing}
          ${this.hasMore ? html`<button class="btn btn-sm mt-4" ?disabled=${this.loading} @click=${() => this.#load()}>Load more</button>` : nothing}
        </section>
        <aside
          class="lg:w-72 shrink-0 lg:flex lg:flex-col lg:min-h-0 lg:max-h-full"
          aria-label="Media inspector"
        >
          ${
            file
              ? html` <button
                    class="btn btn-sm mb-3 lg:shrink-0 lg:self-start"
                    ?disabled=${!file.abs_path}
                    @click=${() => void this.#openInApp(file)}
                  >
                    <omb-icon name="external-link"></omb-icon
                    >${openInAppLabel(snapshot.settings, snapshot.computer.os, file)}
                  </button>
                  <omb-media-inspector
                    class="lg:flex lg:flex-col lg:flex-1 lg:min-h-0"
                    .file=${file}
                  ></omb-media-inspector>`
              : html`<p class="text-sm text-base-content/60">
                  Select a file to inspect its embedded metadata.
                </p>`
          }
        </aside>
      </div>
      ${this.creatingProject ? html`<p class="text-sm" role="status"><span class="loading loading-spinner loading-sm"></span> Reading selected files' embedded capture times…</p>` : nothing}
      ${this.selectionError ? html`<p class="text-sm text-warning" role=${this.selectionError.startsWith("Could not") ? "alert" : "status"}>${this.selectionError}</p>` : nothing}
      ${this.selection.selected.size && !range && !this.selectionError && !this.creatingProject ? html`<p class="text-sm text-base-content/60" role="status">Selected files need embedded capture dates and known timezones. Create project from selection checks them. Unknown dates are not inferred.</p>` : nothing}
    </div>`;
    const actions = html` <button class="btn btn-ghost" @click=${() => this.dismiss()}>Close</button>
      <button
        class="btn btn-primary"
        ?disabled=${!this.selection.selected.size || this.creatingProject}
        @click=${() => void this.#createProjectFromSelection()}
      >
        ${this.creatingProject ? "Reading capture times…" : "Create project from selection"}
      </button>`;
    return html`<omb-modal
      size="xl"
      heading="Browse media"
      subheading=${deviceById(snapshot, source.device_id)?.name ?? "Source"}
      icon="images"
      .body=${body}
      bodyClass="px-6 py-5 flex-1 min-h-0 overflow-y-auto lg:overflow-hidden lg:flex lg:flex-col"
      .actions=${actions}
      @close=${this.onClosed}
    ></omb-modal>`;
  }
}
