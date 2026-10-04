import type {
  AnalysisFilter,
  AnalysisJob,
  AnalysisMetrics,
  AnalysisSummary,
  AnalysisTotals,
} from "../api/types";
import { formatBytes } from "./format";

export function emptyAnalysisMetrics(): AnalysisMetrics {
  return {
    queued_secs: 0,
    other_secs: 0,
    copy_secs: 0,
    source_check_secs: 0,
    destination_check_secs: 0,
    paused_secs: 0,
    decision_secs: 0,
    copy_bytes: 0,
    committed_bytes: 0,
    source_check_bytes: 0,
    destination_check_bytes: 0,
    transferred_files: 0,
    adopted_files: 0,
    skipped_files: 0,
  };
}

export function emptyAnalysisTotals(): AnalysisTotals {
  return {
    metrics: emptyAnalysisMetrics(),
    completed_transfer_jobs: 0,
    completed_check_jobs: 0,
    failed_jobs: 0,
    cancelled_jobs: 0,
    interrupted_jobs: 0,
    avg_copy_bps: null,
    effective_bps: null,
  };
}

export function activeSeconds(metrics: AnalysisMetrics): number {
  return metrics.copy_secs + metrics.source_check_secs + metrics.destination_check_secs + metrics.other_secs;
}

export function wallSeconds(metrics: AnalysisMetrics): number {
  return activeSeconds(metrics) + metrics.queued_secs + metrics.paused_secs + metrics.decision_secs;
}

export function analysisMatches(job: AnalysisJob, filter: AnalysisFilter): boolean {
  return (
    (!filter.space_id || job.context.space_id === filter.space_id) &&
    (!filter.pair_id || job.context.pair_id === filter.pair_id) &&
    (filter.since === null || job.created_at >= filter.since)
  );
}

/** Mirrors the backend aggregation for the demo; partial/live jobs never enter averages. */
export function summarizeAnalysis(jobs: AnalysisJob[], filter: AnalysisFilter): AnalysisSummary {
  const pairs = new Map<
    string,
    { context: AnalysisJob["context"]; totals: AnalysisTotals; activeCopySecs: number }
  >();
  const totals = emptyAnalysisTotals();
  let activeCopySecs = 0;
  const add = (sum: AnalysisTotals, job: AnalysisJob) => {
    if (job.state === "done") {
      if (job.kind === "transfer") sum.completed_transfer_jobs++;
      else sum.completed_check_jobs++;
      for (const key of Object.keys(sum.metrics) as (keyof AnalysisMetrics)[])
        sum.metrics[key] += job.metrics[key];
    } else if (job.state === "failed") sum.failed_jobs++;
    else if (job.state === "cancelled") sum.cancelled_jobs++;
    else if (job.state === "interrupted") sum.interrupted_jobs++;
  };
  for (const job of jobs
    .filter((j) => j.finished_at !== null && analysisMatches(j, filter))
    .sort((a, b) => a.created_at - b.created_at || a.id.localeCompare(b.id))) {
    let pair = pairs.get(job.context.pair_id);
    if (!pair) {
      pair = { context: job.context, totals: emptyAnalysisTotals(), activeCopySecs: 0 };
      pairs.set(job.context.pair_id, pair);
    }
    pair.context = job.context;
    add(pair.totals, job);
    add(totals, job);
    if (job.state === "done" && job.kind === "transfer") {
      pair.activeCopySecs += activeSeconds(job.metrics);
      activeCopySecs += activeSeconds(job.metrics);
    }
  }
  const speeds = (sum: AnalysisTotals, seconds: number) => {
    sum.avg_copy_bps =
      sum.metrics.copy_bytes > 0 && sum.metrics.copy_secs > 0
        ? sum.metrics.copy_bytes / sum.metrics.copy_secs
        : null;
    sum.effective_bps =
      sum.metrics.committed_bytes > 0 && seconds > 0 ? sum.metrics.committed_bytes / seconds : null;
  };
  speeds(totals, activeCopySecs);
  return {
    totals,
    pairs: [...pairs.values()]
      .map((pair) => {
        speeds(pair.totals, pair.activeCopySecs);
        return { context: pair.context, totals: pair.totals };
      })
      .sort(
        (a, b) =>
          a.context.source_name.localeCompare(b.context.source_name) ||
          a.context.destination_name.localeCompare(b.context.destination_name) ||
          a.context.pair_id.localeCompare(b.context.pair_id),
      ),
  };
}

export function formatSpeed(bps: number | null): string {
  return bps !== null && bps > 0 && Number.isFinite(bps) ? `${formatBytes(bps)}/s` : "Not applicable";
}

export function formatDuration(seconds: number): string {
  if (seconds > 0 && seconds < 1) return "<1s";
  const whole = Math.max(0, Math.round(seconds));
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const rest = whole % 60;
  return hours ? `${hours}h ${minutes}m ${rest}s` : minutes ? `${minutes}m ${rest}s` : `${rest}s`;
}

export const ANALYSIS_PHASE_LABELS = {
  queued: "Queued",
  other: "Planning / finalizing",
  copy: "Copying",
  source_check: "Local checks (source)",
  destination_check: "Remote checks (destination)",
  paused: "Paused",
  awaiting_decision: "Waiting for decision",
  finished: "Finished",
};
