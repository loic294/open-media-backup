import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import { transferTotals } from "../../state/derived";
import { formatSpeed } from "../../utils/eta";
import { formatBytes, formatEta, percent, plural } from "../../utils/format";
import { OmbElement } from "../ui/omb-element";
import "./active-jobs-panel";

/** Aggregate progress bar; hovering shows each transfer with its own pause button. */
@customElement("omb-transfer-progress")
export class OmbTransferProgress extends OmbElement {
  override render() {
    const t = transferTotals(this.store.transfers);
    const pct = percent(t.bytesDone, t.bytesTotal);
    const idle = t.active.length === 0;
    const checking = t.active.filter((job) => job.kind === "check").length;
    const marking = t.active.filter((job) => job.kind === "app_import").length;
    const waiting = t.active.filter(
      (job) => job.pending_conflict || job.state === "awaiting_decision",
    ).length;
    const label = idle
      ? "No transfers running"
      : waiting
        ? `${plural(waiting, "job")} awaiting a decision`
        : t.paused
          ? "Jobs paused"
          : checking
            ? `${plural(checking, "destination check")} active`
            : marking
              ? `${plural(marking, "import confirmation")} active`
              : t.running
                ? `${plural(t.running, "transfer")} running`
                : "Jobs queued";
    const speed = formatSpeed(t.bytesPerSec);
    const eta = formatEta(t.etaSeconds);
    const details = [
      speed || null,
      eta ? `${eta} left` : null,
      `${formatBytes(t.bytesDone)} of ${formatBytes(t.bytesTotal)}`,
    ].filter(Boolean);
    return html`
      <div class="dropdown dropdown-top dropdown-hover">
        <div
          tabindex="0"
          role="button"
          class="flex items-center gap-4 rounded-box border border-base-300 bg-base-100 px-3 py-2 w-[32rem]"
        >
          <span class="grid place-items-center size-8 rounded-field bg-primary/15 text-primary"
            ><omb-icon name="arrow-left-right"></omb-icon
          ></span>
          <div class="flex-1 min-w-0">
            <div class="flex justify-between text-sm gap-2">
              <span class="truncate"
                ><b>${label}</b>${
                  idle ? nothing : html`<span class="text-base-content/60"> · ${details.join(" · ")}</span>`
                }</span
              >
              ${idle ? nothing : html`<span class="text-primary font-semibold">${pct}%</span>`}
            </div>
            <progress class="progress progress-primary h-1.5 w-full" value=${pct} max="100"></progress>
          </div>
          <button
            class="btn btn-sm btn-square"
            ?disabled=${idle}
            title=${t.paused ? "Resume all" : "Pause all"}
            @click=${() => this.store.setPaused(null, !t.paused)}
          >
            <omb-icon name=${t.paused ? "play" : "pause"}></omb-icon>
          </button>
        </div>
        ${
          idle
            ? nothing
            : html`<div
                tabindex="0"
                class="dropdown-content w-[32rem] rounded-box border border-base-300 bg-base-100 p-4 shadow-xl"
                style="position: fixed; bottom: 5rem; left: 1.25rem; z-index: 1000;"
              >
                <omb-active-jobs-panel data-omb-block></omb-active-jobs-panel>
              </div>`
        }
      </div>
    `;
  }
}
