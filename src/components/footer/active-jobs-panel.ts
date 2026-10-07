import { html, nothing } from "lit";
import { customElement, property } from "lit/decorators.js";
import type { TransferJob } from "../../api/types";
import { transferTotals } from "../../state/derived";
import { formatSpeed, liveTransferSpeed } from "../../utils/eta";
import { formatBytes, formatEta, percent, plural } from "../../utils/format";
import { inDesktopShell } from "../../api";
import { openActiveJobsWindow } from "../../desktop/active-jobs-window";
import { OmbElement } from "../ui/omb-element";

@customElement("omb-active-jobs-panel")
export class OmbActiveJobsPanel extends OmbElement {
  @property({ type: Boolean })
  standalone = false;

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
              >${waiting ? "Waiting for your decision" : paused ? "Paused" : job.state === "queued" ? "Queued" : job.kind === "check" ? "Checking hashes" : job.kind === "app_import" ? "Marking as transferred" : speed || `${pct}%`}</span
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
            ${job.remote_hash_active ? html` · NAS-side hash checks` : nothing}
          </div>
          ${(job.warnings ?? []).map((warning) => html`<p class="text-xs text-warning break-words">${warning}</p>`)}
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

  async #openWindow() {
    try {
      await openActiveJobsWindow();
    } catch (error) {
      console.error("Could not open Active jobs window", error);
      this.store.toast("error", `Could not open Active jobs window: ${error}`);
    }
  }

  override render() {
    const t = transferTotals(this.store.transfers);
    const jobs = t.active;
    return html`
      <section class=${this.standalone ? "flex h-full min-h-0 flex-col p-5" : ""}>
        <div class="flex items-center justify-between mb-1">
          <h3 class="font-semibold">Active jobs</h3>
          <div class="flex items-center gap-1">
            ${
              !this.standalone && inDesktopShell()
                ? html`<button
                    type="button"
                    class="btn btn-xs btn-ghost btn-square"
                    title="Open Active jobs in a new window"
                    aria-label="Open Active jobs in a new window"
                    @click=${() => this.#openWindow()}
                  >
                    <omb-icon name="external-link" class="size-4"></omb-icon>
                  </button>`
                : nothing
            }
            ${
              jobs.length
                ? html`<button
                    class="btn btn-xs btn-ghost gap-1"
                    @click=${() => this.store.setPaused(null, !t.paused)}
                  >
                    <omb-icon name=${t.paused ? "play" : "pause"} class="size-3"></omb-icon
                    >${t.paused ? "Resume all" : "Pause all"}
                  </button>`
                : nothing
            }
          </div>
        </div>
        ${
          jobs.length
            ? html`<ul class="divide-y divide-base-300 ${this.standalone ? "flex-1 overflow-y-auto" : ""}">
                ${jobs.map((job) => this.#job(job))}
              </ul>`
            : this.standalone
              ? html`<p class="py-6 text-center text-sm text-base-content/60">No active jobs</p>`
              : nothing
        }
      </section>
    `;
  }
}
