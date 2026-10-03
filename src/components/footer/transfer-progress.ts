import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import type { TransferJob } from "../../api/types";
import { transferTotals } from "../../state/derived";
import { formatBytes, formatEta, percent, plural } from "../../utils/format";
import { OmbElement } from "../ui/omb-element";

/** Aggregate progress bar; hovering shows each transfer with its own pause button. */
@customElement("omb-transfer-progress")
export class OmbTransferProgress extends OmbElement {
  #job(job: TransferJob) {
    const pct = percent(job.bytes_done, job.bytes_total);
    const paused = job.state === "paused";
    return html`
      <li class="flex items-center gap-3 py-2">
        <div class="flex-1 min-w-0">
          <div class="flex justify-between text-sm gap-3">
            <span class="font-medium truncate">${job.label}</span>
            <span class="text-base-content/60 whitespace-nowrap">${paused ? "Paused" : job.state === "queued" ? "Queued" : `${pct}%`}</span>
          </div>
          <progress class="progress ${paused ? "" : "progress-primary"} h-1.5 w-full" value=${pct} max="100"></progress>
          <div class="text-xs text-base-content/50 truncate">
            ${job.files_done}/${plural(job.files_total, "file")} · ${formatBytes(job.bytes_done)} of ${formatBytes(job.bytes_total)}
            ${job.current_file ? html` · ${job.current_file}` : nothing}
          </div>
        </div>
        <button class="btn btn-ghost btn-xs btn-square" title=${paused ? "Resume" : "Pause"} @click=${() => this.store.setPaused(job.id, !paused)}>
          <omb-icon name=${paused ? "play" : "pause"}></omb-icon>
        </button>
      </li>
    `;
  }

  override render() {
    const t = transferTotals(this.store.transfers);
    const pct = percent(t.bytesDone, t.bytesTotal);
    const idle = t.active.length === 0;
    const label = idle ? "No transfers running" : t.paused ? "Transfers paused" : `${plural(t.running, "transfer")} running`;
    return html`
      <div class="dropdown dropdown-top dropdown-hover">
        <div tabindex="0" role="button" class="flex items-center gap-4 rounded-box border border-base-300 bg-base-100 px-3 py-2 w-[28rem]">
          <span class="grid place-items-center size-8 rounded-field bg-primary/15 text-primary"><omb-icon name="arrow-left-right"></omb-icon></span>
          <div class="flex-1 min-w-0">
            <div class="flex justify-between text-sm gap-2">
              <span class="truncate"><b>${label}</b>${idle ? nothing : html`<span class="text-base-content/60"> · ${formatBytes(t.bytesDone)} of ${formatBytes(t.bytesTotal)}${t.etaSeconds ? ` · ${formatEta(t.etaSeconds)} left` : ""}</span>`}</span>
              ${idle ? nothing : html`<span class="text-primary font-semibold">${pct}%</span>`}
            </div>
            <progress class="progress progress-primary h-1.5 w-full" value=${pct} max="100"></progress>
          </div>
          <button class="btn btn-sm btn-square" ?disabled=${idle} title=${t.paused ? "Resume all" : "Pause all"} @click=${() => this.store.setPaused(null, !t.paused)}>
            <omb-icon name=${t.paused ? "play" : "pause"}></omb-icon>
          </button>
        </div>
        ${idle
          ? nothing
          : html`<div tabindex="0" class="dropdown-content z-30 mb-2 w-[28rem] rounded-box border border-base-300 bg-base-100 p-4 shadow-xl">
              <div class="flex items-center justify-between mb-1">
                <h3 class="font-semibold">Active transfers</h3>
                <button class="btn btn-xs btn-ghost gap-1" @click=${() => this.store.setPaused(null, !t.paused)}>
                  <omb-icon name=${t.paused ? "play" : "pause"} class="size-3"></omb-icon>${t.paused ? "Resume all" : "Pause all"}
                </button>
              </div>
              <ul class="divide-y divide-base-300">${t.active.map((j) => this.#job(j))}</ul>
            </div>`}
      </div>
    `;
  }
}
