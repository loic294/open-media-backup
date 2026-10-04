import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import { projectTotals } from "../../state/derived";
import { spaceFlows } from "../../state/selectors";
import { formatCount, plural } from "../../utils/format";
import { OmbElement } from "../ui/omb-element";
import { hashVerificationSummary, hashVerificationTooltip } from "./hash-verification";
import "./transfer-progress";

@customElement("omb-footer")
export class OmbFooter extends OmbElement {
  override render() {
    const { snapshot, space, status } = this.store;
    const totals = projectTotals(
      status,
      snapshot && space ? spaceFlows(snapshot, space.id) : [],
      snapshot?.destinations ?? [],
    );
    return html`
      <footer class="relative z-50 flex items-center gap-5 px-5 py-3 border-t border-base-300 bg-base-200">
        <omb-transfer-progress></omb-transfer-progress>
        ${space ? this.#verificationStatus(space.hash_algo, space.verify_mode, totals.errors) : nothing}
        <span class="flex-1"></span>
        <button
          class="btn gap-2"
          ?disabled=${!space}
          @click=${() => this.store.open({ type: "preview", flowId: null })}
        >
          <omb-icon name="images"></omb-icon>Preview
        </button>
        <button
          class="btn btn-primary gap-2"
          ?disabled=${!totals.runnable}
          @click=${() => this.store.runAll()}
        >
          <omb-icon name="play"></omb-icon>Run all transfers
          ${totals.runnable ? html`<span class="badge badge-sm bg-primary-content/20 border-0 text-primary-content">${formatCount(totals.runnable)}</span>` : nothing}
        </button>
      </footer>
    `;
  }

  #verificationStatus(
    hashAlgo: NonNullable<typeof this.store.space>["hash_algo"],
    verifyMode: NonNullable<typeof this.store.space>["verify_mode"],
    failed: number,
  ) {
    return html`<span class="flex items-center gap-2 text-sm">
      <span class="tooltip tooltip-top tooltip-center">
        <span
          id="hash-verification-tooltip"
          role="tooltip"
          class="tooltip-content z-50 rounded-box bg-neutral p-3 text-left text-neutral-content shadow-lg"
        >
          <span class="block font-semibold">Verification details</span>
          ${hashVerificationTooltip(hashAlgo, verifyMode, failed)
            .split("\n")
            .map((line) => html`<span class="block whitespace-nowrap">${line}</span>`)}
        </span>
        <button
          type="button"
          class="btn btn-sm btn-outline min-h-8 h-8 gap-2 px-3 cursor-help"
          aria-label=${hashVerificationSummary(hashAlgo, verifyMode)}
          aria-describedby="hash-verification-tooltip"
        >
          <omb-icon name="shield-check" class="text-success"></omb-icon>
          <span>Verified</span>
        </button>
      </span>
      ${
        failed
          ? html`<button
              type="button"
              class="btn btn-ghost btn-sm min-h-8 h-8 px-2 text-error"
              @click=${() => this.store.open({ type: "preview", flowId: null, category: "error" })}
            >
              ${plural(failed, "failed file")}
            </button>`
          : nothing
      }
    </span>`;
  }
}
