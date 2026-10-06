import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { DestinationCheckItem } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { DialogBase } from "./dialog-base";

const OUTCOME_LABEL: Record<DestinationCheckItem["outcome"], string> = {
  matched: "Hash matches",
  verified: "Catalog hash verified",
  untracked: "Untracked / no comparable catalog hash",
  missing: "Missing",
  conflict: "Different hash",
  error: "Could not check",
};
const OUTCOME_ORDER: Record<DestinationCheckItem["outcome"], number> = {
  error: 0,
  conflict: 1,
  missing: 2,
  matched: 3,
  verified: 4,
  untracked: 3,
};

@customElement("omb-destination-check-results-dialog")
export class OmbDestinationCheckResultsDialog extends DialogBase<
  Extract<DialogRequest, { type: "destination-check-results" }>
> {
  @state() private visibleCount = 100;

  override render() {
    const job = this.request.job;
    const result = job.check_results;
    const items = [...(result?.items ?? [])].sort(
      (a, b) => OUTCOME_ORDER[a.outcome] - OUTCOME_ORDER[b.outcome],
    );
    return html`<omb-modal
      heading=${job.state === "cancelled" ? "Destination check cancelled" : "Destination check results"}
      subheading=${job.label}
      icon="fingerprint"
      size="lg"
      @close=${this.onClosed}
      .body=${html`
        <p class="text-sm text-base-content/70 mb-4">
          ${
            job.flow_id.startsWith("check:destination:")
              ? "Destination contents were inventoried against recorded catalog evidence. Catalog verification is not a fresh source comparison."
              : "Expected destination paths were compared with configured sources."
          }
          No media files were copied or changed.
          ${job.state === "cancelled" ? "These are partial results." : ""}
        </p>
        ${
          result
            ? html`<p class="font-semibold mb-4">
                ${result.matched} matching · ${result.missing} missing · ${result.conflicts} different ·
                ${result.errors} errors · ${result.verified ?? 0} catalog verified · ${result.untracked ?? 0}
                untracked
              </p>`
            : nothing
        }
        <p class="text-xs text-base-content/60 mb-3">
          Matching and catalog-verified files are counted above. Untracked files have no comparable evidence;
          they are not a pass. Only paths needing attention are listed below.
        </p>
        ${job.errors.map((error) => html`<p class="text-sm text-error break-all mb-2">${error}</p>`)}
        <ul class="divide-y divide-base-300">
          ${items.slice(0, this.visibleCount).map(
            (item) => html`
              <li class="py-3 text-sm">
                <p
                  class="font-semibold ${item.outcome === "error" ? "text-error" : item.outcome === "conflict" ? "text-warning" : ""}"
                >
                  ${OUTCOME_LABEL[item.outcome]}
                </p>
                <p class="break-all">${item.destination_path}</p>
                ${
                  item.source_path
                    ? html`<p class="text-xs text-base-content/60 break-all">Source: ${item.source_path}</p>`
                    : nothing
                }
                ${item.error ? html`<p class="text-error break-all">${item.error}</p>` : nothing}
              </li>
            `,
          )}
        </ul>
        ${
          items.length > this.visibleCount
            ? html`
                <button
                  class="btn btn-sm mt-3"
                  @click=${() => {
                    this.visibleCount += 100;
                  }}
                >
                  Show more (${items.length - this.visibleCount} remaining)
                </button>
              `
            : nothing
        }
      `}
      .actions=${html`<button class="btn" @click=${() => this.dismiss()}>Close</button>`}
    ></omb-modal>`;
  }
}
