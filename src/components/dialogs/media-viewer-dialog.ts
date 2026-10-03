import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { FileEntry } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { DialogBase } from "./dialog-base";
import "./media-inspector";
import "../ui/omb-thumbnail";

@customElement("omb-media-viewer-dialog")
export class OmbMediaViewerDialog extends DialogBase<Extract<DialogRequest, { type: "media-viewer" }>> {
  @state() private file!: FileEntry;
  @state() private src: string | null = null;
  @state() private loading = false;
  @state() private error: string | null = null;
  #seq = 0;

  override connectedCallback(): void {
    super.connectedCallback();
    this.file = this.request.file;
    void this.#load();
  }

  override disconnectedCallback(): void {
    this.#seq++;
    this.#releaseSrc();
    super.disconnectedCallback();
  }

  get #files(): FileEntry[] {
    const files = this.request.files ?? [];
    return files.some((file) => file.rel_path === this.request.file.rel_path)
      ? files
      : [this.request.file, ...files];
  }

  #releaseSrc() {
    if (this.src?.startsWith("blob:")) URL.revokeObjectURL(this.src);
  }

  #selectFile(file: FileEntry) {
    if (file.rel_path === this.file.rel_path) return;
    this.file = file;
    void this.#load();
  }

  async #openDefault() {
    if (!this.file.abs_path) return;
    try {
      await this.store.backend.openMedia(this.file.abs_path);
      this.store.toast("success", `Opened ${this.file.name} in the default app`);
    } catch (error) {
      this.store.toast("error", `Could not open ${this.file.name}: ${error}`);
    }
  }

  async #load() {
    const seq = ++this.#seq;
    this.#releaseSrc();
    this.src = null;
    this.error = null;
    const path = this.file?.abs_path;
    if (!path) {
      this.error = "The source file is not available for preview.";
      return;
    }
    this.loading = true;
    try {
      const src = await this.store.backend.mediaPreview(path, this.file.media);
      if (seq !== this.#seq) {
        if (src?.startsWith("blob:")) URL.revokeObjectURL(src);
        return;
      }
      this.src = src;
      if (!src) this.error = "No in-app preview is available for this file.";
    } catch (error) {
      if (seq === this.#seq) {
        this.error = `Could not load preview: ${error}`;
        this.store.toast("error", this.error);
      }
    } finally {
      if (seq === this.#seq) this.loading = false;
    }
  }

  override render() {
    const file = this.file ?? this.request.file;
    const body = html`<div class="flex flex-col gap-4">
      <div class="flex flex-col lg:flex-row gap-5">
        <section
          aria-label="Large in-app preview"
          aria-busy=${this.loading}
          class="flex-1 min-w-0 min-h-64 rounded-box bg-neutral text-neutral-content grid place-items-center p-4"
        >
          ${this.loading ? html`<span role="status" aria-label="Loading preview" class="loading loading-spinner"></span>` : nothing}
          ${
            this.src && file.media === "video"
              ? html`<video
                  class="max-h-[60vh] max-w-full object-contain"
                  controls
                  preload="metadata"
                  src=${this.src}
                  @error=${() => {
                    this.error = "The in-app preview could not play this video.";
                    this.store.toast("error", this.error);
                  }}
                >
                  Your browser cannot play this video format.
                </video>`
              : this.src
                ? html`<img
                    class="max-h-[60vh] max-w-full object-contain"
                    src=${this.src}
                    alt=${file.name}
                    @error=${() => {
                      this.#releaseSrc();
                      this.src = null;
                      this.error = "The in-app preview could not be displayed.";
                      this.store.toast("error", this.error);
                    }}
                  />`
                : nothing
          }
          ${this.error ? html`<p role="alert" class="text-sm">${this.error}</p>` : nothing}
        </section>
        <aside class="lg:w-72 shrink-0"><omb-media-inspector .file=${file}></omb-media-inspector></aside>
      </div>
      ${
        this.#files.length > 1
          ? html`<nav class="flex items-start gap-2 overflow-x-auto border-t border-base-300 pt-3" aria-label="Media thumbnails">
              ${this.#files.map((item) => html`<button
                type="button"
                class="w-24 shrink-0 rounded-field border p-1 text-left ${item.rel_path === file.rel_path ? "border-primary ring-2 ring-primary/40" : "border-base-300"}"
                aria-label=${`Preview ${item.name}`}
                aria-pressed=${item.rel_path === file.rel_path}
                @click=${() => this.#selectFile(item)}
              >
                <omb-thumbnail data-omb-block .file=${item}></omb-thumbnail>
              </button>`)}
            </nav>`
          : nothing
      }
      <p class="text-xs text-base-content/60">
        Media is previewed in-app when the file format is supported by this webview.
      </p>
    </div>`;
    const actions = html`
      ${this.error && file.abs_path ? html`<button class="btn" @click=${() => this.#load()}>Retry preview</button>` : nothing}
      <button class="btn" ?disabled=${!file.abs_path} @click=${() => this.#openDefault()}>
        <omb-icon name="external-link"></omb-icon>Open in default app
      </button>
      <button class="btn" @click=${() => this.dismiss()}>Back to gallery</button>
    `;
    return html`<omb-modal
      size="xl"
      heading=${file.name}
      subheading="In-app media viewer"
      icon="maximize"
      .body=${body}
      .actions=${actions}
      @close=${this.onClosed}
    ></omb-modal>`;
  }
}
