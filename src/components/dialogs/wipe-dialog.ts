import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { WipeMethod, WipePlan } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { deviceById } from "../../state/selectors";
import { formatCount, percent } from "../../utils/format";
import { DialogBase } from "./dialog-base";

@customElement("omb-wipe-dialog")
export class OmbWipeDialog extends DialogBase<Extract<DialogRequest, { type: "wipe-card" }>> {
  @state() private plan: WipePlan | null = null;
  @state() private method: WipeMethod = "delete_files";
  @state() private confirmed = false;
  @state() private busy = false;

  override connectedCallback(): void {
    super.connectedCallback();
    const project = this.store.project;
    if (project) {
      this.store.backend
        .planWipe(project.id, this.request.sourceId)
        .then((plan) => (this.plan = plan))
        .catch((e) => this.store.toast("error", String(e)));
    }
  }

  async #wipe() {
    const project = this.store.project;
    if (!project) return;
    this.busy = true;
    try {
      await this.store.backend.wipe(project.id, this.request.sourceId, this.method);
      this.store.toast("success", "Wipe started. Every file is re-verified before anything is deleted.");
      this.dismiss();
    } catch (e) {
      this.store.toast("error", String(e));
    } finally {
      this.busy = false;
    }
  }

  #methods() {
    const options: [WipeMethod, string, string][] = [
      ["delete_files", "Delete backed-up files", "Removes only the files that are verified on the final destinations. Unknown files and folders stay. Recommended."],
      ["quick_format", "Quick format (exFAT)", "Erases the whole card, including files that were never backed up. Best done in the camera for optimal performance."],
    ];
    return options.map(
      ([value, title, text]) => html`<label class="flex items-start gap-3 rounded-box border p-3 cursor-pointer ${this.method === value ? "border-primary bg-primary/5" : "border-base-300"}">
        <input type="radio" name="omb-wipe" class="radio radio-sm radio-primary mt-0.5" .checked=${this.method === value} @change=${() => (this.method = value)} />
        <span><span class="font-medium">${title}</span><span class="block text-sm text-base-content/60">${text}</span></span>
      </label>`,
    );
  }

  override render() {
    if (!this.store.project) return html`<omb-modal
      heading="Wipe card"
      icon="eraser"
      @close=${this.onClosed}
      .body=${html`<div role="alert" class="alert alert-warning alert-soft">
        <span>Create a project to set card-wiping safety requirements.</span>
      </div>`}
      .actions=${html`<button class="btn btn-ghost" @click=${() => this.dismiss()}>Close</button>`}
    ></omb-modal>`;
    const source = this.store.snapshot?.sources.find((s) => s.id === this.request.sourceId);
    const name = source ? deviceById(this.store.snapshot!, source.device_id)?.name : "";
    const p = this.plan;
    const body = !p
      ? html`<div class="grid place-items-center py-10"><span class="loading loading-spinner"></span></div>`
      : html`
          <div class="flex flex-col gap-4">
            <p class="text-sm">${formatCount(p.files_total)} files on ${name}${p.ignored ? html` · <span class="text-base-content/60">${formatCount(p.ignored)} ignored by rules</span>` : nothing}</p>
            <ul class="flex flex-col gap-2">
              ${p.copies.map(
                (c) => html`<li class="flex items-center gap-3">
                  <omb-icon name=${c.verified >= c.total && c.total > 0 ? "shield-check" : "shield-alert"} class=${c.verified >= c.total && c.total > 0 ? "text-success" : "text-warning"}></omb-icon>
                  <span class="w-40 truncate">${c.device_name}</span>
                  <progress class="progress ${c.verified >= c.total ? "progress-success" : "progress-warning"} flex-1" value=${percent(c.verified, c.total)} max="100"></progress>
                  <span class="text-sm text-base-content/60 w-28 text-right">${formatCount(c.verified)} / ${formatCount(c.total)}</span>
                </li>`,
              )}
            </ul>
            ${p.eligible
              ? html`
                  <div class="flex flex-col gap-2">${this.#methods()}</div>
                  <div role="alert" class="alert alert-warning alert-soft text-sm">
                    <omb-icon name="triangle-alert"></omb-icon>
                    <span>Before deleting, every file is hashed again and compared with the catalog. If anything changed, nothing is deleted.</span>
                  </div>
                  <label class="flex items-center gap-2 text-sm cursor-pointer">
                    <input type="checkbox" class="checkbox checkbox-sm checkbox-error" .checked=${this.confirmed} @change=${(e: Event) => (this.confirmed = (e.target as HTMLInputElement).checked)} />
                    I understand this can't be undone
                  </label>
                `
              : html`<div role="alert" class="alert alert-error alert-soft text-sm"><omb-icon name="circle-alert"></omb-icon><span>Not safe to wipe yet: ${p.reason}</span></div>`}
          </div>
        `;
    const actions = html`
      <button class="btn btn-ghost" @click=${() => this.dismiss()}>Cancel</button>
      <button class="btn btn-error" ?disabled=${!p?.eligible || !this.confirmed || this.busy} @click=${() => this.#wipe()}>
        ${this.busy ? html`<span class="loading loading-spinner loading-sm"></span>` : html`<omb-icon name="eraser"></omb-icon>`}Wipe ${name}
      </button>
    `;
    return html`<omb-modal heading="Wipe card" subheading=${name ?? ""} icon="eraser" @close=${this.onClosed} .body=${body} .actions=${actions}></omb-modal>`;
  }
}
