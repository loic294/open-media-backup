import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type {
  AnalysisContext,
  AnalysisFilter,
  AnalysisJobPage,
  AnalysisMetrics,
  AnalysisSummary,
  AnalysisTotals,
  TransferJob,
} from "../../api/types";
import { formatBytes, formatCount, percent, plural } from "../../utils/format";
import {
  activeSeconds,
  ANALYSIS_PHASE_LABELS,
  formatDuration,
  formatSpeed,
  wallSeconds,
} from "../../utils/speed-analysis";
import { OmbElement } from "../ui/omb-element";

const PAGE_SIZE = 5;
const terminal = (job: TransferJob) => ["done", "failed", "cancelled"].includes(job.state);
const pairLabel = (context: AnalysisContext) => `${context.source_name} → ${context.destination_name}`;

@customElement("omb-speed-analysis")
export class OmbSpeedAnalysis extends OmbElement {
  @state() private summary: AnalysisSummary | null = null;
  @state() private summaryLoading = true;
  @state() private summaryError: string | null = null;
  @state() private spaceId = "";
  @state() private days = "30";
  @state() private pairFilter = "";
  @state() private selectedPairId = "";
  @state() private history: AnalysisJobPage | null = null;
  @state() private historyLoading = false;
  @state() private historyError: string | null = null;
  @state() private offset = 0;
  @state() private limit = PAGE_SIZE;
  @state() private expandedJobId = "";
  @state() private polledJobs: TransferJob[] | null = null;
  @state() private liveError: string | null = null;
  #summarySeq = 0;
  #historySeq = 0;
  #liveSeq = 0;
  #livePending = false;
  #since: number | null = Date.now() - 30 * 86_400_000;
  #terminalSignature = "";
  #timer: ReturnType<typeof setInterval> | undefined;
  #knownSpaces = new Map<string, string>();
  #knownPairs = new Map<string, AnalysisContext>();

  #remember(context: AnalysisContext): void {
    this.#knownSpaces.set(context.space_id, context.space_name);
    this.#knownPairs.set(context.pair_id, context);
  }

  override connectedCallback(): void {
    super.connectedCallback();
    for (const job of this.store.transfers) if (job.analysis) this.#remember(job.analysis.context);
    this.#terminalSignature = this.#signature();
    this.store.addEventListener("change", this.#onStoreChange);
    void this.#loadSummary();
    this.#timer = setInterval(() => {
      if (this.store.transfers.some((job) => !terminal(job) && job.analysis)) void this.#pollLive();
    }, 1000);
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this.store.removeEventListener("change", this.#onStoreChange);
    clearInterval(this.#timer);
    this.#summarySeq++;
    this.#historySeq++;
    this.#liveSeq++;
  }

  #signature(): string {
    return this.store.transfers
      .filter(terminal)
      .map((job) => `${job.id}:${job.state}`)
      .sort()
      .join("|");
  }

  #onStoreChange = () => {
    this.polledJobs = null;
    for (const job of this.store.transfers) if (job.analysis) this.#remember(job.analysis.context);
    const signature = this.#signature();
    if (signature !== this.#terminalSignature) {
      this.#terminalSignature = signature;
      void this.#loadSummary();
    }
  };

  #filter(pairId: string | null = null): AnalysisFilter {
    return { space_id: this.spaceId || null, since: this.#since, pair_id: pairId };
  }

  async #loadSummary(): Promise<void> {
    const seq = ++this.#summarySeq;
    this.summaryLoading = true;
    this.summaryError = null;
    try {
      const summary = await this.store.backend.getSpeedAnalysis(this.#filter());
      if (!this.isConnected || seq !== this.#summarySeq) return;
      for (const pair of summary.pairs) this.#remember(pair.context);
      this.summary = summary;
      const pairs = summary.pairs.filter((p) => !this.pairFilter || p.context.pair_id === this.pairFilter);
      if (!pairs.some((p) => p.context.pair_id === this.selectedPairId)) {
        this.selectedPairId = pairs[0]?.context.pair_id ?? "";
        this.offset = 0;
        this.expandedJobId = "";
      }
      if (this.selectedPairId) void this.#loadHistory();
      else this.history = null;
    } catch (error) {
      if (!this.isConnected || seq !== this.#summarySeq) return;
      this.summaryError = String(error);
    } finally {
      if (this.isConnected && seq === this.#summarySeq) this.summaryLoading = false;
    }
  }

  async #loadHistory(): Promise<void> {
    const seq = ++this.#historySeq;
    if (!this.selectedPairId) {
      this.historyLoading = false;
      this.historyError = null;
      return;
    }
    this.historyLoading = true;
    this.historyError = null;
    try {
      const history = await this.store.backend.listSpeedAnalysisJobs({
        ...this.#filter(this.selectedPairId),
        offset: this.offset,
        limit: this.limit,
      });
      if (!this.isConnected || seq !== this.#historySeq) return;
      if (this.offset >= history.total && this.offset > 0) {
        this.offset = Math.max(0, Math.floor((history.total - 1) / this.limit) * this.limit);
        void this.#loadHistory();
        return;
      }
      this.history = history;
    } catch (error) {
      if (this.isConnected && seq === this.#historySeq) this.historyError = String(error);
    } finally {
      if (this.isConnected && seq === this.#historySeq) this.historyLoading = false;
    }
  }

  async #pollLive(): Promise<void> {
    if (this.#livePending) return;
    this.#livePending = true;
    const seq = ++this.#liveSeq;
    const original = this.store.transfers;
    try {
      const jobs = await this.store.backend.listTransfers();
      if (!this.isConnected || seq !== this.#liveSeq) return;
      this.liveError = null;
      if (original === this.store.transfers) {
        for (const job of jobs) if (job.analysis) this.#remember(job.analysis.context);
        this.polledJobs = jobs;
        if (jobs.some((job) => terminal(job) && !original.some((old) => old.id === job.id && terminal(old))))
          void this.#loadSummary();
      }
    } catch (error) {
      if (this.isConnected && seq === this.#liveSeq) this.liveError = String(error);
    } finally {
      this.#livePending = false;
    }
  }

  #changeFilters(event: Event, field: "space" | "days"): void {
    const value = (event.target as HTMLSelectElement).value;
    if (field === "space") this.spaceId = value;
    else {
      this.days = value;
      this.#since = value === "all" ? null : Date.now() - Number(value) * 86_400_000;
    }
    this.pairFilter = "";
    this.selectedPairId = "";
    this.offset = 0;
    this.history = null;
    this.summary = null;
    this.#historySeq++;
    void this.#loadSummary();
  }

  #selectPair(pairId: string): void {
    this.selectedPairId = pairId;
    this.offset = 0;
    this.history = null;
    this.expandedJobId = "";
    void this.#loadHistory();
  }

  #changePairFilter(event: Event): void {
    this.pairFilter = (event.target as HTMLSelectElement).value;
    const pairs = this.summary?.pairs.filter(
      (p) => !this.pairFilter || p.context.pair_id === this.pairFilter,
    );
    this.#selectPair(pairs?.[0]?.context.pair_id ?? "");
  }

  #error(label: string, error: string, retry: () => void) {
    return html`<div role="alert" class="alert alert-error alert-soft alert-vertical sm:alert-horizontal">
      <div class="min-w-0 flex-1">
        <p class="font-medium">${label}</p>
        <p class="text-xs break-words">${error}</p>
      </div>
      <button class="btn btn-sm" type="button" @click=${retry}>Retry</button>
    </div>`;
  }

  #stats(totals: AnalysisTotals) {
    return html`<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
      ${[
        ["Average copy speed", formatSpeed(totals.avg_copy_bps), "Physical writes / active copy time"],
        [
          "Data copied",
          formatBytes(totals.metrics.committed_bytes),
          "Committed files; excludes adoption / skips",
        ],
        [
          "Separate hash checks",
          formatDuration(
            totals.metrics.source_check_secs +
              totals.metrics.destination_check_secs +
              totals.metrics.remote_check_secs,
          ),
          "Source reads + destination re-reads + NAS hashes",
        ],
        [
          "Completed jobs",
          formatCount(totals.completed_transfer_jobs + totals.completed_check_jobs),
          `${plural(totals.completed_transfer_jobs, "transfer")} · ${plural(totals.completed_check_jobs, "check")}`,
        ],
      ].map(
        ([label, value, desc]) =>
          html`<div class="stats bg-base-200/50 border border-base-300 min-w-0">
            <div class="stat p-4 min-w-0">
              <div class="stat-title text-xs">${label}</div>
              <div class="stat-value text-2xl break-words whitespace-normal">${value}</div>
              <div class="stat-desc text-xs whitespace-normal">${desc}</div>
            </div>
          </div>`,
      )}
    </div>`;
  }

  #breakdown(metrics: AnalysisMetrics, id: string) {
    const active = activeSeconds(metrics);
    const phases = [
      {
        label: "Copy + inline hash",
        seconds: metrics.copy_secs,
        bytes: metrics.copy_bytes,
        color: "bg-primary",
      },
      {
        label: "Local checks (source)",
        seconds: metrics.source_check_secs,
        bytes: metrics.source_check_bytes,
        color: "bg-info",
      },
      {
        label: "Destination re-reads",
        seconds: metrics.destination_check_secs,
        bytes: metrics.destination_check_bytes,
        color: "bg-success",
      },
      {
        label: "NAS-side hash checks",
        seconds: metrics.remote_check_secs,
        bytes: metrics.remote_check_bytes,
        color: "bg-warning",
      },
      { label: "Other active work", seconds: metrics.other_secs, bytes: null, color: "bg-neutral" },
    ];
    return html`<div class="flex flex-col gap-3">
      ${
        active > 0
          ? html`<div
              role="img"
              aria-label=${phases.map((p) => `${p.label}: ${((p.seconds / active) * 100).toFixed(1)}%`).join(", ")}
              class="flex h-2 rounded-box overflow-hidden bg-base-300"
            >
              ${phases.map((p) => html`<span class=${p.color} style=${`width: ${(p.seconds / active) * 100}%`}></span>`)}
            </div>`
          : nothing
      }
      <dl id=${id} class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        ${phases.map(
          (p) =>
            html`<div>
              <dt class="text-xs text-base-content/60">${p.label}</dt>
              <dd class="font-semibold text-xl">${formatDuration(p.seconds)}</dd>
              <dd class="text-xs text-base-content/60">
                ${active > 0 ? `${((p.seconds / active) * 100).toFixed(1)}% of active time` : "No active time recorded"}
                ${p.bytes !== null ? html` · ${formatBytes(p.bytes)} ${p.label.startsWith("Copy") ? "written" : "read"}` : nothing}
              </dd>
            </div>`,
        )}
      </dl>
      <p class="text-xs text-base-content/60">
        Active: ${formatDuration(active)} · Wall time: ${formatDuration(wallSeconds(metrics))} · Queued:
        ${formatDuration(metrics.queued_secs)} · Paused: ${formatDuration(metrics.paused_secs)} · Decisions:
        ${formatDuration(metrics.decision_secs)}
      </p>
      <p class="text-xs text-base-content/60">
        ${formatCount(metrics.transferred_files)} copied files · ${formatCount(metrics.adopted_files)} adopted
        · ${formatCount(metrics.skipped_files)} skipped · ${formatBytes(metrics.committed_bytes)} committed
      </p>
    </div>`;
  }

  #live(jobs: TransferJob[]) {
    return html`<section aria-label="Live jobs" class="flex flex-col gap-3">
      <div class="flex items-center justify-between">
        <h4 class="font-medium">Live now</h4>
        <span class="badge badge-ghost badge-sm">${jobs.length} active</span>
      </div>
      ${this.liveError ? this.#error("Could not refresh live metrics", this.liveError, () => void this.#pollLive()) : nothing}
      ${
        jobs.length
          ? jobs.map((job) => {
              const analysis = job.analysis!;
              const m = analysis.metrics;
              const rate =
                analysis.phase === "copy"
                  ? m.copy_secs > 0
                    ? m.copy_bytes / m.copy_secs
                    : null
                  : analysis.phase === "source_check"
                    ? m.source_check_secs > 0
                      ? m.source_check_bytes / m.source_check_secs
                      : null
                    : analysis.phase === "destination_check"
                      ? m.destination_check_secs > 0
                        ? m.destination_check_bytes / m.destination_check_secs
                        : null
                      : analysis.phase === "remote_check"
                        ? m.remote_check_secs > 0
                          ? m.remote_check_bytes / m.remote_check_secs
                          : null
                        : null;
              return html`<article
                class="rounded-box bg-base-200/50 border border-base-300 p-4 flex flex-col gap-2"
                data-live-job=${job.id}
              >
                <div class="flex flex-col sm:flex-row gap-3 sm:items-center sm:justify-between">
                  <div class="min-w-0">
                    <h5 class="font-medium break-words">${pairLabel(analysis.context)}</h5>
                    <p class="text-xs text-base-content/60">
                      ${analysis.context.space_name} · ${ANALYSIS_PHASE_LABELS[analysis.phase]} ·
                      ${job.kind === "check" ? "Check job" : "Transfer"}
                      ${job.current_file ? html` · ${job.current_file}` : nothing}
                    </p>
                  </div>
                  <div>
                    <div class="text-xl font-semibold">${formatSpeed(rate)}</div>
                    <div class="text-xs text-base-content/60">Phase average so far</div>
                  </div>
                  <div class="text-sm">
                    ${formatBytes(job.bytes_done)} / ${formatBytes(job.bytes_total)} ·
                    ${percent(job.bytes_done, job.bytes_total)}%
                    <div class="text-xs text-base-content/60">
                      Copy ${formatDuration(m.copy_secs)} · Checks
                      ${formatDuration(m.source_check_secs + m.destination_check_secs + m.remote_check_secs)}
                    </div>
                  </div>
                </div>
                <progress
                  class="progress progress-success w-full"
                  aria-label=${`${pairLabel(analysis.context)} progress`}
                  value=${job.bytes_done}
                  max=${Math.max(1, job.bytes_total)}
                ></progress>
                <details>
                  <summary class="cursor-pointer text-xs">Timing details</summary>
                  <div class="pt-3">${this.#breakdown(m, `live-metrics-${job.id}`)}</div>
                </details>
              </article>`;
            })
          : html`<p class="text-sm text-base-content/60">No active transfers or checks for these filters.</p>`
      }
      <p class="text-xs text-base-content/60">
        Live jobs update automatically and stay separate from recorded averages. The date range applies to
        history only.
      </p>
    </section>`;
  }

  #jobHistory() {
    return html`<section class="flex flex-col gap-3" aria-label="Job history">
      <div class="flex items-center justify-between gap-3">
        <h4 class="font-medium">Recent jobs for this pair</h4>
        <button
          type="button"
          class="btn btn-sm"
          ?disabled=${this.historyLoading}
          @click=${() => {
            this.limit = this.limit === PAGE_SIZE ? 25 : PAGE_SIZE;
            this.offset = 0;
            void this.#loadHistory();
          }}
        >
          ${this.limit === PAGE_SIZE ? "All jobs" : "Recent jobs"}
        </button>
      </div>
      ${
        this.historyError
          ? this.#error("Could not load job history", this.historyError, () => void this.#loadHistory())
          : this.historyLoading
            ? html`<p role="status" class="text-sm">Loading job history…</p>`
            : this.history?.jobs.length
              ? html`<div class="overflow-x-auto">
                  <table class="table table-sm">
                    <thead>
                      <tr>
                        <th>Started</th>
                        <th>Outcome</th>
                        <th>Kind</th>
                        <th>Committed</th>
                        <th>Copy average</th>
                        <th>Copy / hash time</th>
                        <th>Details</th>
                      </tr>
                    </thead>
                    <tbody>
                      ${this.history.jobs.map(
                        (job) =>
                          html`<tr>
                            <td>
                              <time datetime=${new Date(job.created_at).toISOString()}
                                >${new Date(job.created_at).toLocaleString()}</time
                              >
                            </td>
                            <td>
                              <span
                                class="badge badge-sm ${job.state === "done" ? "badge-success badge-soft" : job.state === "failed" ? "badge-error badge-soft" : "badge-ghost"}"
                                >${job.state === "done" ? "Completed" : job.state}</span
                              >
                            </td>
                            <td>${job.kind}</td>
                            <td>${formatBytes(job.metrics.committed_bytes)}</td>
                            <td>
                              ${formatSpeed(job.metrics.copy_secs > 0 ? job.metrics.copy_bytes / job.metrics.copy_secs : null)}
                            </td>
                            <td>
                              ${formatDuration(job.metrics.copy_secs)} /
                              ${formatDuration(job.metrics.source_check_secs + job.metrics.destination_check_secs + job.metrics.remote_check_secs)}
                            </td>
                            <td>
                              <button
                                type="button"
                                class="btn btn-ghost btn-xs"
                                aria-expanded=${this.expandedJobId === job.id}
                                aria-controls=${`job-detail-${job.id}`}
                                @click=${() => (this.expandedJobId = this.expandedJobId === job.id ? "" : job.id)}
                              >
                                Details
                              </button>
                            </td>
                          </tr>`,
                      )}
                    </tbody>
                  </table>
                </div>`
              : html`<p class="text-sm text-base-content/60">
                  No recorded jobs for this pair in this date range.
                </p>`
      }
      ${
        !this.historyLoading && !this.historyError
          ? this.history?.jobs.map(
              (job) =>
                html` <div
                  id=${`job-detail-${job.id}`}
                  ?hidden=${this.expandedJobId !== job.id}
                  class="rounded-box border border-base-300 bg-base-200/30 p-4 flex flex-col gap-3"
                >
                  <p class="text-xs">
                    Verification: ${job.context.verify_mode} · Hash: ${job.context.hash_algo} · Errors:
                    ${job.error_count}
                    ${job.state === "interrupted" ? "· Last durable checkpoint; app downtime is excluded." : job.state !== "done" ? "· Partial metrics; excluded from completed-job averages." : ""}
                  </p>
                  ${this.#breakdown(job.metrics, `job-metrics-${job.id}`)}
                </div>`,
            )
          : nothing
      }
      ${
        this.history && !this.historyError
          ? html`<div class="flex justify-between items-center gap-3 text-xs">
              <span
                >${this.history.total ? `${this.offset + 1}–${Math.min(this.offset + this.limit, this.history.total)} of ${this.history.total} jobs` : "0 jobs"}</span
              >
              <div class="flex gap-2">
                <button
                  class="btn btn-sm"
                  type="button"
                  ?disabled=${this.offset === 0 || this.historyLoading}
                  @click=${() => {
                    this.offset -= this.limit;
                    void this.#loadHistory();
                  }}
                >
                  Previous
                </button>
                <button
                  class="btn btn-sm"
                  type="button"
                  ?disabled=${this.offset + this.limit >= this.history.total || this.historyLoading}
                  @click=${() => {
                    this.offset += this.limit;
                    void this.#loadHistory();
                  }}
                >
                  Next
                </button>
              </div>
            </div>`
          : nothing
      }
    </section>`;
  }

  override render() {
    const liveJobs = (this.polledJobs ?? this.store.transfers).filter(
      (job) =>
        !terminal(job) &&
        job.analysis &&
        (!this.spaceId || job.analysis.context.space_id === this.spaceId) &&
        (!this.pairFilter || job.analysis.context.pair_id === this.pairFilter),
    );
    const spaces = new Map(this.#knownSpaces);
    for (const space of this.store.snapshot?.spaces ?? []) spaces.set(space.id, space.name);
    const options = [...this.#knownPairs].filter(
      ([, context]) => !this.spaceId || context.space_id === this.spaceId,
    );
    const pairs =
      this.summary?.pairs.filter((p) => !this.pairFilter || p.context.pair_id === this.pairFilter) ?? [];
    const selected = pairs.find((p) => p.context.pair_id === this.selectedPairId);
    const totals = this.pairFilter ? pairs[0]?.totals : this.summary?.totals;
    return html`<div class="flex flex-col gap-5">
      <div>
        <h4 class="text-lg font-semibold">Speed Analysis</h4>
        <p class="text-sm text-base-content/60">Find where time goes across your backup routes.</p>
      </div>
      <div class="flex flex-col sm:flex-row flex-wrap gap-3 sm:items-end">
        <label class="flex flex-col gap-1 text-xs"
          >Space<select
            class="select select-sm"
            aria-label="Analysis space"
            .value=${this.spaceId}
            @change=${(e: Event) => this.#changeFilters(e, "space")}
          >
            <option value="">All spaces</option>
            ${[...spaces].map(([id, name]) => html`<option value=${id}>${name}</option>`)}
          </select></label
        >
        <label class="flex flex-col gap-1 text-xs"
          >Recorded history<select
            class="select select-sm"
            aria-label="Analysis date range"
            .value=${this.days}
            @change=${(e: Event) => this.#changeFilters(e, "days")}
          >
            <option value="7">Last 7 days</option>
            <option value="30">Last 30 days</option>
            <option value="all">All recorded</option>
          </select></label
        >
        <label class="flex flex-col gap-1 text-xs"
          >Source → destination<select
            class="select select-sm max-w-full sm:max-w-80"
            aria-label="Analysis pair"
            .value=${this.pairFilter}
            @change=${(e: Event) => this.#changePairFilter(e)}
          >
            <option value="">All pairs</option>
            ${options.map(([id, context]) => html`<option value=${id}>${pairLabel(context)} · ${context.space_name} · ${context.source_device_id.slice(0, 8)} / ${context.destination_device_id.slice(0, 8)}</option>`)}
          </select></label
        >
        <span class="text-xs text-base-content/60 sm:ml-auto">History on this computer only</span>
      </div>
      ${this.#live(liveJobs)}
      ${
        this.summaryError
          ? this.#error("Could not load speed history", this.summaryError, () => void this.#loadSummary())
          : !this.summary && this.summaryLoading
            ? html`<p role="status">Loading speed history…</p>`
            : !pairs.length
              ? html`<div class="rounded-box border border-base-300 bg-base-200/30 p-6">
                  <h4 class="font-semibold">
                    ${this.pairFilter || this.spaceId || this.days !== "all" ? "No recorded jobs for these filters" : "No speed history yet"}
                  </h4>
                  <p class="text-sm text-base-content/60">
                    Run a transfer or a destination check to start collecting statistics, or select All
                    recorded.
                  </p>
                </div>`
              : html`
                  ${this.summaryLoading ? html`<p role="status" class="text-xs text-base-content/60">Refreshing recorded statistics…</p>` : nothing}
                  ${totals ? this.#stats(totals) : nothing}
                  <section class="flex flex-col gap-3">
                    <h4 class="font-medium">Source to destination</h4>
                    <div class="overflow-x-auto">
                      <table class="table table-sm">
                        <thead>
                          <tr>
                            <th>Pair</th>
                            <th>Transfer / check jobs</th>
                            <th>Copied</th>
                            <th>Avg copy</th>
                            <th>Copy / hash time</th>
                            <th>Details</th>
                          </tr>
                        </thead>
                        <tbody>
                          ${pairs.map(
                            (pair) =>
                              html`<tr
                                class=${pair.context.pair_id === this.selectedPairId ? "bg-base-200" : ""}
                              >
                                <td>
                                  <div class="font-medium">${pairLabel(pair.context)}</div>
                                  <div class="text-xs text-base-content/60">
                                    ${pair.context.space_name} · ${pair.context.source_device_name} →
                                    ${pair.context.destination_device_name}
                                  </div>
                                </td>
                                <td>
                                  ${pair.totals.completed_transfer_jobs} / ${pair.totals.completed_check_jobs}
                                </td>
                                <td>${formatBytes(pair.totals.metrics.committed_bytes)}</td>
                                <td>${formatSpeed(pair.totals.avg_copy_bps)}</td>
                                <td>
                                  ${formatDuration(pair.totals.metrics.copy_secs)} /
                                  ${formatDuration(pair.totals.metrics.source_check_secs + pair.totals.metrics.destination_check_secs + pair.totals.metrics.remote_check_secs)}
                                </td>
                                <td>
                                  <button
                                    type="button"
                                    class="btn btn-ghost btn-xs"
                                    aria-pressed=${pair.context.pair_id === this.selectedPairId}
                                    @click=${() => this.#selectPair(pair.context.pair_id)}
                                  >
                                    View
                                  </button>
                                </td>
                              </tr>`,
                          )}
                        </tbody>
                      </table>
                    </div>
                  </section>
                  ${
                    selected
                      ? html`<section
                            class="rounded-box border border-base-300 bg-base-200/30 p-4 flex flex-col gap-3"
                          >
                            <h4 class="font-medium">${pairLabel(selected.context)}</h4>
                            ${this.#breakdown(selected.totals.metrics, "pair-time-breakdown")}
                            <p class="text-xs text-base-content/60">
                              Effective transfer throughput: ${formatSpeed(selected.totals.effective_bps)} ·
                              ${selected.totals.failed_jobs} failed · ${selected.totals.cancelled_jobs}
                              cancelled · ${selected.totals.interrupted_jobs} interrupted
                            </p>
                            <p class="text-xs text-base-content/60">
                              Durations are summed job time, not unique wall-clock time when jobs overlap.
                              Check-only jobs contribute check time, not transfer averages.
                            </p>
                          </section>
                          ${this.#jobHistory()}`
                      : nothing
                  }
                `
      }
      <div class="text-xs text-base-content/60 flex flex-col gap-1">
        <p>
          Source checks and destination re-reads read bytes on this computer (including over SMB/NFS).
          NAS-side checks read on the hash server and return only a digest; their bytes are server work, not
          network re-read traffic.
        </p>
        <p>
          Inline source hashing is included in copy time, not counted twice. Copy speed uses physical writes
          (including retries); copied data counts only committed files.
        </p>
        <p>
          Collection starts with this feature; older jobs are not backfilled. All recorded history is retained
          locally across restarts and is not synced to peers.
        </p>
        <p>
          Live, failed, cancelled and interrupted jobs are excluded from completed-job averages. Decimal
          units: 1 MB = 1,000,000 bytes.
        </p>
      </div>
    </div>`;
  }
}
