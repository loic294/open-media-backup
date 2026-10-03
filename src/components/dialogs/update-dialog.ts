import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { UpdateProgress } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { installAvailableUpdate } from "../../state/updater";
import { formatUpdateProgress, parseReleaseNotes, updateProgressPercent } from "../../utils/update";
import { DialogBase } from "./dialog-base";

type UpdateDialogState = "prompt" | "downloading" | "restarting" | "error";

@customElement("omb-update-dialog")
export class OmbUpdateDialog extends DialogBase<Extract<DialogRequest, { type: "update" }>> {
  @state() private mode: UpdateDialogState = "prompt";
  @state() private progress: UpdateProgress = { downloaded: 0, total: null };
  @state() private error = "";

  #later() {
    this.dismiss();
  }

  async #install() {
    this.mode = "downloading";
    this.error = "";
    this.progress = { downloaded: 0, total: null };
    try {
      await installAvailableUpdate(this.store, (progress) => {
        this.progress = progress;
      });
      this.mode = "restarting";
    } catch (error) {
      this.error = String(error);
      this.mode = "error";
    }
  }

  #notes() {
    const blocks = parseReleaseNotes(this.request.update.notes);
    if (!blocks.length) return html`<p class="text-base-content/60">No release notes were provided.</p>`;
    return html`${blocks.map((block) =>
      block.kind === "bullets"
        ? html`<ul class="list-disc space-y-2 pl-5">
            ${block.items.map((item) => html`<li>${item}</li>`)}
          </ul>`
        : html`<p class="whitespace-pre-line">${block.text}</p>`,
    )}`;
  }

  #promptBody() {
    return html`
      <div class="flex flex-col gap-6">
        <section class="rounded-box border border-base-300 bg-base-200/35 p-5">
          <h4 class="mb-3 font-semibold">Release notes</h4>
          <div class="space-y-3 text-sm text-base-content/80">${this.#notes()}</div>
        </section>
        ${
          this.error
            ? html`<div role="alert" class="alert alert-error alert-soft">
                <omb-icon name="circle-alert" class="size-5"></omb-icon><span>${this.error}</span>
              </div>`
            : nothing
        }
      </div>
    `;
  }

  #progressBody() {
    const percent = updateProgressPercent(this.progress.downloaded, this.progress.total);
    const label = formatUpdateProgress(this.progress.downloaded, this.progress.total);
    return html`
      <div class="flex flex-col gap-6">
        <div>
          <div class="mb-3 flex items-center justify-between gap-4 text-sm text-base-content/70">
            <span>${this.mode === "restarting" ? "Download complete" : label}</span>
            ${percent == null ? nothing : html`<span>${percent}%</span>`}
          </div>
          ${
            percent == null
              ? html`<progress class="progress progress-primary w-full"></progress>`
              : html`<progress
                  class="progress progress-primary w-full"
                  value=${percent}
                  max="100"
                ></progress>`
          }
        </div>
        <p class="text-sm text-base-content/70">
          ${this.mode === "restarting" ? "Restarting…" : "Keep working safely while the update downloads."}
        </p>
        ${
          this.error
            ? html`<div role="alert" class="alert alert-error alert-soft">
                <omb-icon name="circle-alert" class="size-5"></omb-icon><span>${this.error}</span>
              </div>`
            : nothing
        }
      </div>
    `;
  }

  override render() {
    const update = this.request.update;
    const busy = this.mode === "downloading" || this.mode === "restarting";
    const heading =
      this.mode === "prompt"
        ? `Update available: Open Media Backup ${update.version}`
        : this.mode === "error"
          ? "Update failed"
          : this.mode === "restarting"
            ? "Restarting…"
            : "Downloading update…";
    const subheading =
      this.mode === "prompt"
        ? `You have ${update.current_version} installed`
        : this.mode === "error"
          ? "The update could not be installed. You can try again now."
          : formatUpdateProgress(this.progress.downloaded, this.progress.total);
    const icon = this.mode === "prompt" ? "arrow-up" : this.mode === "error" ? "circle-alert" : "arrow-down";
    const actions =
      this.mode === "prompt"
        ? html`<button class="btn btn-ghost" type="button" @click=${() => this.#later()}>Later</button>
            <button class="btn btn-primary" type="button" @click=${() => this.#install()}>
              Update and restart
            </button>`
        : this.mode === "error"
          ? html`<button class="btn btn-ghost" type="button" @click=${() => this.#later()}>Close</button>
              <button class="btn btn-primary" type="button" @click=${() => this.#install()}>Retry</button>`
          : html`<button class="btn" type="button" disabled>
              ${this.mode === "restarting" ? "Restarting…" : "Installing…"}
            </button>`;

    return html`<omb-modal
      size="lg"
      heading=${heading}
      subheading=${subheading}
      icon=${icon}
      bodyClass="px-6 py-6 overflow-y-auto flex-1 min-h-0"
      .closeable=${!busy}
      @close=${this.onClosed}
      .body=${this.mode === "prompt" || this.mode === "error" ? this.#promptBody() : this.#progressBody()}
      .actions=${actions}
    ></omb-modal>`;
  }
}
