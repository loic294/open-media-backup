import { describe, expect, it } from "vitest";
import type { AnalysisJob } from "../api/types";
import {
  activeSeconds,
  emptyAnalysisMetrics,
  formatDuration,
  formatSpeed,
  summarizeAnalysis,
  wallSeconds,
} from "./speed-analysis";

const filter = { space_id: null, since: null, pair_id: null };
function job(id: string, changes: Partial<AnalysisJob> = {}): AnalysisJob {
  return {
    id,
    kind: "transfer",
    state: "done",
    phase: "finished",
    created_at: 1000,
    updated_at: 2000,
    finished_at: 2000,
    error_count: 0,
    context: {
      pair_id: "pair",
      space_id: "space",
      space_name: "Space",
      source_id: "source",
      destination_id: "destination",
      source_device_id: "card",
      destination_device_id: "ssd",
      source_name: "Card",
      destination_name: "SSD",
      source_device_name: "Card",
      destination_device_name: "SSD",
      hash_algo: "blake3",
      verify_mode: "reread",
    },
    metrics: emptyAnalysisMetrics(),
    ...changes,
  };
}

describe("speed analysis calculations", () => {
  it("weights speeds by copy seconds and excludes checks and incomplete outcomes from transfer throughput", () => {
    const first = job("first", {
      metrics: {
        ...emptyAnalysisMetrics(),
        copy_bytes: 100,
        committed_bytes: 100,
        copy_secs: 1,
        source_check_secs: 1,
      },
    });
    const second = job("second", {
      metrics: {
        ...emptyAnalysisMetrics(),
        copy_bytes: 100,
        committed_bytes: 80,
        copy_secs: 9,
        destination_check_secs: 2,
        other_secs: 1,
      },
    });
    const check = job("check", {
      kind: "check",
      metrics: { ...emptyAnalysisMetrics(), source_check_secs: 100, destination_check_secs: 100 },
    });
    const failed = job("failed", {
      state: "failed",
      metrics: { ...emptyAnalysisMetrics(), copy_bytes: 9999, copy_secs: 1 },
    });
    const live = job("live", { state: "running", phase: "copy", finished_at: null, metrics: failed.metrics });
    const totals = summarizeAnalysis([first, second, check, failed, live], filter).totals;
    expect(totals.avg_copy_bps).toBe(20);
    expect(totals.effective_bps).toBe(180 / 14);
    expect(totals.metrics.source_check_secs).toBe(101);
    expect(totals.completed_transfer_jobs).toBe(2);
    expect(totals.completed_check_jobs).toBe(1);
    expect(totals.failed_jobs).toBe(1);
  });

  it("keeps check-only and cancelled pairs inspectable without inventing copy speeds", () => {
    const summary = summarizeAnalysis(
      [
        job("check", {
          kind: "check",
          metrics: { ...emptyAnalysisMetrics(), source_check_bytes: 100, source_check_secs: 1 },
        }),
        job("cancelled", { state: "cancelled" }),
        job("interrupted", { state: "interrupted" }),
      ],
      filter,
    );
    expect(summary.pairs).toHaveLength(1);
    expect(summary.totals.avg_copy_bps).toBeNull();
    expect(summary.totals.effective_bps).toBeNull();
    expect(summary.totals.cancelled_jobs).toBe(1);
    expect(summary.totals.interrupted_jobs).toBe(1);
    expect(formatSpeed(null)).toBe("Not applicable");
    expect(formatSpeed(0)).toBe("Not applicable");
  });

  it("filters timestamps and stable IDs instead of names, using the latest name snapshot", () => {
    const older = job("old", { created_at: 1000 });
    const newer = job("new", {
      created_at: 3000,
      context: { ...older.context, source_name: "Renamed card" },
    });
    const different = job("different", {
      context: { ...older.context, pair_id: "other", space_id: "other-space" },
    });
    expect(
      summarizeAnalysis([older, newer, different], { space_id: "space", pair_id: "pair", since: 2000 }).totals
        .completed_transfer_jobs,
    ).toBe(1);
    expect(summarizeAnalysis([newer, older], filter).pairs[0].context.source_name).toBe("Renamed card");
    expect(summarizeAnalysis([older, newer, different], filter).pairs).toHaveLength(2);
  });

  it("separates active work from queued, paused and decision time and formats durations", () => {
    const m = {
      ...emptyAnalysisMetrics(),
      copy_secs: 10,
      source_check_secs: 3,
      destination_check_secs: 5,
      other_secs: 2,
      queued_secs: 7,
      paused_secs: 8,
      decision_secs: 9,
    };
    expect(activeSeconds(m)).toBe(20);
    expect(wallSeconds(m)).toBe(44);
    expect(formatDuration(0.25)).toBe("<1s");
    expect(formatDuration(3661)).toBe("1h 1m 1s");
    expect(formatSpeed(50_000_000)).toBe("50.0 MB/s");
  });
});
