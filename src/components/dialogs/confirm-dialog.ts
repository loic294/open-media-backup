import { html } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { DialogRequest } from "../../state/dialogs";
import { DialogBase } from "./dialog-base";

@customElement("omb-confirm-dialog")
export class OmbConfirmDialog extends DialogBase<Extract<DialogRequest, { type: "confirm" }>> {
  @state() private busy = false;

  async #confirm() {
    this.busy = true;
    try {
      await this.request.onConfirm();
    } finally {
      this.busy = false;
      this.dismiss();
    }
  }

  override render() {
    const r = this.request;
    return html`<omb-modal
      size="sm"
      heading=${r.title}
      icon=${r.danger ? "triangle-alert" : "info"}
      @close=${this.onClosed}
      .body=${html`<p class="text-sm text-base-content/70">${r.message}</p>`}
      .actions=${html`
        <button class="btn btn-ghost" @click=${() => this.dismiss()}>Cancel</button>
        <button class="btn ${r.danger ? "btn-error" : "btn-primary"}" ?disabled=${this.busy} @click=${() => this.#confirm()}>
          ${this.busy ? html`<span class="loading loading-spinner loading-sm"></span>` : null}${r.confirmLabel}
        </button>
      `}
    ></omb-modal>`;
  }
}
