import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { ConflictDecision } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { DialogBase } from "./dialog-base";

@customElement("omb-transfer-conflict-dialog")
export class OmbTransferConflictDialog extends DialogBase<
  Extract<DialogRequest, { type: "transfer-conflict" }>
> {
  @state() private busy = false;
  @state() private applyToRemaining = false;

  async #decide(decision: ConflictDecision) {
    this.busy = true;
    try {
      await this.store.resolveTransferConflict(
        this.request.jobId,
        this.request.requestId,
        decision,
        this.applyToRemaining,
      );
    } finally {
      this.busy = false;
    }
  }

  async #cancel() {
    this.busy = true;
    try {
      await this.store.cancelTransfer(this.request.jobId);
    } finally {
      this.busy = false;
    }
  }

  override render() {
    const job = this.store.transfers.find((job) => job.id === this.request.jobId);
    const conflict = job?.pending_conflict;
    if (!conflict || conflict.request_id !== this.request.requestId) return nothing;
    return html`<omb-modal
      heading="A different file already exists"
      subheading=${job.label}
      icon="triangle-alert"
      .closeable=${false}
      .body=${html`
        <p class="text-sm mb-4">
          These files have the same destination name but different hashes. Choose what to do.
        </p>
        <dl class="text-sm space-y-3">
          <div>
            <dt class="font-semibold">Source</dt>
            <dd class="break-all">${conflict.source_path}</dd>
            <dd class="font-mono text-xs text-base-content/60 break-all">${conflict.source_hash}</dd>
          </div>
          <div>
            <dt class="font-semibold">Destination</dt>
            <dd class="break-all">${conflict.destination_path}</dd>
            <dd class="font-mono text-xs text-base-content/60 break-all">${conflict.destination_hash}</dd>
          </div>
        </dl>
        <p class="text-sm text-base-content/70 my-4">
          Skip leaves this file pending. Keep both copies it under a numbered filename. Replace overwrites the
          destination only after the new copy is verified.
        </p>
        <label class="flex items-center gap-3 text-sm">
          <input
            type="checkbox"
            class="checkbox checkbox-sm"
            .checked=${this.applyToRemaining}
            ?disabled=${this.busy}
            @change=${(event: Event) => {
              this.applyToRemaining = (event.target as HTMLInputElement).checked;
            }}
          />
          Apply to all remaining conflicts in this queue
        </label>
        <p class="text-xs text-base-content/60 mt-2">This choice will not affect future runs.</p>
      `}
      .actions=${html`
        <div class="flex flex-wrap justify-end gap-2">
          <button class="btn btn-ghost" ?disabled=${this.busy} @click=${() => this.#cancel()}>
            Cancel transfer
          </button>
          <button class="btn" ?disabled=${this.busy} @click=${() => this.#decide("skip")}>Skip</button>
          <button class="btn" ?disabled=${this.busy} @click=${() => this.#decide("keep_both")}>
            Keep both
          </button>
          <button
            class="btn btn-error btn-outline"
            ?disabled=${this.busy}
            @click=${() => this.#decide("replace")}
          >
            Replace
          </button>
        </div>
      `}
    ></omb-modal>`;
  }
}
