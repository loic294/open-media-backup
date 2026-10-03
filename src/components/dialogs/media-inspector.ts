import { html, nothing, type PropertyValues } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import type { FileEntry, MediaMetadata } from "../../api/types";
import { formatBytes } from "../../utils/format";
import { OmbElement } from "../ui/omb-element";
import { captureLabel } from "./media-browser-data";

@customElement("omb-media-inspector")
export class OmbMediaInspector extends OmbElement {
  @property({ attribute: false }) file!: FileEntry;
  @state() private metadata: MediaMetadata | null = null;
  @state() private previewSrc: string | null = null;
  @state() private loading = false;
  @state() private error: string | null = null;
  @state() private previewError: string | null = null;
  #seq = 0;

  protected override willUpdate(changes: PropertyValues): void {
    if (changes.has("file")) void this.#load();
  }

  override disconnectedCallback(): void {
    this.#seq++;
    super.disconnectedCallback();
  }

  async #load() {
    const seq = ++this.#seq;
    this.metadata = null;
    this.previewSrc = null;
    this.error = null;
    this.previewError = null;
    this.loading = false;
    if (!this.file.abs_path) {
      this.error = "The source file is not available for metadata inspection.";
      this.previewError = "The source file is not available for preview.";
      return;
    }
    this.loading = true;
    const [metadata, preview] = await Promise.allSettled([
      this.store.backend.getMediaMetadata(this.file.abs_path),
      this.store.backend.thumbnail(this.file.abs_path),
    ]);
    if (seq !== this.#seq) return;
    if (metadata.status === "fulfilled") this.metadata = metadata.value;
    else {
      this.error = `Could not read embedded metadata: ${metadata.reason}`;
      this.store.toast("error", this.error);
    }
    if (preview.status === "fulfilled") {
      this.previewSrc = preview.value;
      if (!preview.value) this.previewError = "No preview is available for this file.";
    } else {
      this.previewError = `Could not load preview: ${preview.reason}`;
      this.store.toast("error", this.previewError);
    }
    this.loading = false;
  }

  override render() {
    if (!this.file) return nothing;
    const metadata = this.metadata;
    const rows: [string, string | number | null | undefined][] = [
      ["Path", this.file.rel_path],
      ["Size", formatBytes(metadata?.size_bytes ?? this.file.size)],
      ["Eligible capture (UTC)", captureLabel(this.file)],
    ];
    if (metadata) {
      const { capture_time: capture, dimensions, camera, exposure, video } = metadata;
      rows.push(
        ["Embedded capture", capture?.local_datetime ?? "Absent"],
        [
          "Capture timezone",
          capture
            ? capture.utc_offset_seconds === null
              ? "Unknown offset; not eligible for a project range"
              : `UTC offset ${capture.utc_offset_seconds} seconds`
            : "Unavailable",
        ],
        ["Capture source", capture?.source.replaceAll("_", " ")],
        ["Dimensions", dimensions ? `${dimensions.width} × ${dimensions.height}` : null],
        ["Camera make", camera.make],
        ["Camera model", camera.model],
        ["Lens make", camera.lens_make],
        ["Lens model", camera.lens_model],
        ["Shutter", exposure.shutter_seconds == null ? null : `${exposure.shutter_seconds} s`],
        ["Aperture", exposure.aperture_f_number == null ? null : `f/${exposure.aperture_f_number}`],
        ["ISO", exposure.iso],
        ["Focal length", exposure.focal_length_mm == null ? null : `${exposure.focal_length_mm} mm`],
        ["35 mm equivalent", exposure.focal_length_35mm == null ? null : `${exposure.focal_length_35mm} mm`],
        ["Exposure compensation", exposure.compensation_ev == null ? null : `${exposure.compensation_ev} EV`],
        ["Orientation", metadata.orientation],
      );
      if (video)
        rows.push(
          ["Container", video.container],
          ["Codec", video.codec],
          ["Pixel format", video.pixel_format],
          ["Duration", video.duration_seconds == null ? null : `${video.duration_seconds} s`],
          ["Frame rate", video.frame_rate == null ? null : `${video.frame_rate} fps`],
          ["Bit rate", video.bit_rate_bps == null ? null : `${video.bit_rate_bps} bps`],
          ["Rotation", video.rotation_degrees == null ? null : `${video.rotation_degrees}°`],
          [
            "Audio",
            video.audio
              .map((audio) =>
                [
                  audio.codec ?? "Unknown codec",
                  audio.channels == null ? null : `${audio.channels} channels`,
                  audio.sample_rate_hz == null ? null : `${audio.sample_rate_hz} Hz`,
                ]
                  .filter(Boolean)
                  .join(" · "),
              )
              .join("; ") || "None",
          ],
        );
    }
    return html` <div class="flex flex-col gap-3">
      <section class="card card-border bg-base-200 overflow-hidden" aria-label="Selected media preview">
        <div class="aspect-[3/2] bg-base-300 grid place-items-center relative">
          ${
            this.previewSrc
              ? html`<img src=${this.previewSrc} alt=${`Preview of ${this.file.name}`} class="absolute inset-0 size-full object-contain" />`
              : html`<div class="flex flex-col items-center gap-2 p-4 text-center text-base-content/50">
                  <omb-icon name=${this.file.media === "video" ? "film" : "image"} class="size-8"></omb-icon>
                  <span class="text-xs">${this.previewError ?? (this.loading ? "Loading preview…" : "Preview unavailable")}</span>
                </div>`
          }
        </div>
        <div class="flex items-center justify-between gap-2 px-3 py-2 text-xs">
          <span class="truncate">${this.file.name}</span>
          ${this.file.media === "video" ? html`<span class="badge badge-xs badge-neutral">VIDEO</span>` : nothing}
        </div>
      </section>
      <section class="card card-border bg-base-200">
        <div class="card-body p-4 gap-3">
        <h4 class="card-title text-sm">Embedded metadata</h4>
        ${this.loading ? html`<p role="status"><span class="loading loading-spinner loading-sm"></span> Reading metadata…</p>` : nothing}
        ${
          this.error
            ? html`<p role="alert" class="text-sm text-error">${this.error}</p>
                <button class="btn btn-sm" ?disabled=${!this.file.abs_path} @click=${() => this.#load()}>
                  Retry metadata
                </button>`
            : nothing
        }
        <dl class="text-xs flex flex-col gap-2">
          ${rows.map(
            ([label, value]) =>
              html`<div>
                <dt class="text-base-content/60">${label}</dt>
                <dd class="break-words">${value ?? "Unavailable"}</dd>
              </div>`,
          )}
        </dl>
        </div>
      </section>
    </div>`;
  }
}
