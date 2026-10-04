import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import type { TransferJob } from "../../api/types";
import { transferTotals } from "../../state/derived";
import { formatSpeed, liveTransferSpeed } from "../../utils/eta";
import { formatBytes, formatEta, percent, plural } from "../../utils/format";
import { OmbElement } from "../ui/omb-element";

/** Aggregate progress bar; hovering shows each transfer with its own pause button. */
@customElement("omb-transfer-progress")
export class OmbTransferProgress extends OmbElement {
  #job(job: TransferJob) {
    const pct = percent(job.bytes_done, job.bytes_total);
    const paused = job.state === "paused";
    const waiting = !!job.pending_conflict || job.state === "awaiting_decision";
    const speed = paused || waiting ? "" : formatSpeed(liveTransferSpeed(job));
    const eta = paused || waiting ? "" : formatEta(job.eta_secs);
    return html`
      <li class="flex items-center gap-3 py-2">
        <div class="flex-1 min-w-0">
          <div class="flex justify-between text-sm gap-3">
            <span class="font-medium truncate">${job.label}</span>
            <span class="text-base-content/60 whitespace-nowrap"
              >${waiting ? "Waiting for your decision" : paused ? "Paused" : job.state === "queued" ? "Queued" : job.kind === "check" ? "Checking hashes" : speed || `${pct}%`}</span
            >
          </div>
          <progress
            class="progress ${paused ? "" : "progress-primary"} h-1.5 w-full"
            value=${pct}
            max="100"
          ></progress>
          <div class="text-xs text-base-content/50 truncate">
            ${job.files_done}/${plural(job.files_total, "file")} · ${formatBytes(job.bytes_done)} of
            ${formatBytes(job.bytes_total)} ${eta ? html` · ${eta} left` : nothing}
            ${job.current_file ? html` · ${job.current_file}` : nothing}
          </div>
        </div>
        <button
          class="btn btn-ghost btn-xs btn-square"
          title=${paused ? "Resume" : "Pause"}
          ?disabled=${waiting}
          @click=${() => this.store.setPaused(job.id, !paused)}
        >
          <omb-icon name=${paused ? "play" : "pause"}></omb-icon>
        </button>
        <button
          class="btn btn-ghost btn-xs btn-square"
          title="Cancel job"
          @click=${() => this.store.cancelTransfer(job.id)}
        >
          <omb-icon name="x"></omb-icon>
        </button>
      </li>
    `;
  }

  override render() {
    const t = transferTotals(this.store.transfers);
    const pct = percent(t.bytesDone, t.bytesTotal);
    const idle = t.active.length === 0;
    const checking = t.active.filter((job) => job.kind === "check").length;
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
                <div class="flex items-center justify-between mb-1">
                  <h3 class="font-semibold">Active jobs</h3>
                  <button
                    class="btn btn-xs btn-ghost gap-1"
                    @click=${() => this.store.setPaused(null, !t.paused)}
                  >
                    <omb-icon name=${t.paused ? "play" : "pause"} class="size-3"></omb-icon
                    >${t.paused ? "Resume all" : "Pause all"}
                  </button>
                </div>
                <ul class="divide-y divide-base-300">
                  ${t.active.map((j) => this.#job(j))}
                </ul>
              </div>`
        }
      </div>
    `;
  }
}
