import { afterEach, describe, expect, it, vi } from "vitest";
import { createMockBackend } from "./mock-backend";

const filter = { space_id: null, since: null, pair_id: null };

describe("mock speed analysis", () => {
  afterEach(() => vi.useRealTimers());

  it("records waits and terminal history independently from progress snapshots", async () => {
    vi.useFakeTimers();
    const backend = createMockBackend({ tickMs: 400 });
    await backend.runFlow("trip", "f4");
    await vi.advanceTimersByTimeAsync(800);
    const [running] = await backend.listTransfers();
    expect(running.analysis?.metrics.copy_bytes).toBeGreaterThan(0);
    expect((await backend.getSpeedAnalysis(filter)).pairs).toHaveLength(0);
    await backend.setTransferPaused(running.id, true);
    await vi.advanceTimersByTimeAsync(5000);
    const [paused] = await backend.listTransfers();
    expect(paused.analysis?.phase).toBe("paused");
    expect(paused.analysis?.metrics.paused_secs).toBe(5);
    expect(paused.analysis?.metrics.copy_bytes).toBe(running.analysis?.metrics.copy_bytes);
    await backend.setTransferPaused(running.id, false);
    await vi.advanceTimersByTimeAsync(6000);
    const [finished] = await backend.listTransfers();
    expect(finished.state).toBe("done");
    const summary = await backend.getSpeedAnalysis(filter);
    expect(summary.totals.completed_transfer_jobs).toBe(1);
    expect(summary.totals.avg_copy_bps).toBe(
      finished.analysis!.metrics.copy_bytes / finished.analysis!.metrics.copy_secs,
    );
    expect(summary.totals.metrics.paused_secs).toBe(5);
    expect(summary.totals.metrics.committed_bytes).toBe(finished.bytes_total);
    const history = await backend.listSpeedAnalysisJobs({ ...filter, offset: 0, limit: 5 });
    expect(history.total).toBe(1);
    expect(history.jobs[0].id).toBe(finished.id);
    await vi.advanceTimersByTimeAsync(10_000);
    expect((await backend.listSpeedAnalysisJobs({ ...filter, offset: 0, limit: 5 })).jobs).toEqual(
      history.jobs,
    );
  });

  it("keeps cancelled partial measurements inspectable and out of averages", async () => {
    vi.useFakeTimers();
    const backend = createMockBackend({ tickMs: 400 });
    await backend.runFlow("trip", "f4");
    await vi.advanceTimersByTimeAsync(800);
    const [job] = await backend.listTransfers();
    await backend.cancelTransfer(job.id);
    await vi.advanceTimersByTimeAsync(400);
    const summary = await backend.getSpeedAnalysis(filter);
    expect(summary.totals.cancelled_jobs).toBe(1);
    expect(summary.totals.completed_transfer_jobs).toBe(0);
    expect(summary.totals.avg_copy_bps).toBeNull();
    const history = await backend.listSpeedAnalysisJobs({ ...filter, offset: 0, limit: 5 });
    expect(history.jobs[0].metrics.committed_bytes).toBeGreaterThan(0);
    expect(history.jobs[0].state).toBe("cancelled");
  });

  it("does not count skipped conflicts as copied bytes and times decisions separately", async () => {
    vi.useFakeTimers();
    const backend = createMockBackend({ tickMs: 400, conflicts: { f4: 2 } });
    await backend.runFlow("trip", "f4");
    await vi.advanceTimersByTimeAsync(400);
    const [waiting] = await backend.listTransfers();
    expect(waiting.pending_conflict).not.toBeNull();
    await vi.advanceTimersByTimeAsync(5000);
    await backend.resolveTransferConflict(waiting.id, waiting.pending_conflict!.request_id, "skip", true);
    await vi.advanceTimersByTimeAsync(6000);
    const [done] = await backend.listTransfers();
    expect(done.state).toBe("done");
    expect(done.analysis?.metrics.skipped_files).toBe(2);
    expect(done.analysis?.metrics.committed_bytes).toBe(done.bytes_total - 2 * 39_000_000);
    expect(done.analysis?.metrics.decision_secs).toBe(5);
  });

  it("records standalone checks without copy throughput and validates pagination", async () => {
    vi.useFakeTimers();
    const backend = createMockBackend({ tickMs: 400 });
    await backend.checkWorkspaceDestination({ spaceId: "travel", projectId: "trip" }, "d2");
    await vi.advanceTimersByTimeAsync(6000);
    const summary = await backend.getSpeedAnalysis(filter);
    expect(summary.totals.completed_check_jobs).toBe(2);
    expect(summary.totals.completed_transfer_jobs).toBe(0);
    expect(summary.totals.avg_copy_bps).toBeNull();
    expect(summary.totals.metrics.source_check_bytes).toBeGreaterThan(0);
    await expect(backend.listSpeedAnalysisJobs({ ...filter, offset: -1, limit: 5 })).rejects.toThrow(
      "pagination",
    );
    await expect(backend.listSpeedAnalysisJobs({ ...filter, offset: 0, limit: 101 })).rejects.toThrow(
      "pagination",
    );
    expect((await backend.listSpeedAnalysisJobs({ ...filter, offset: 0, limit: 1 })).jobs).toHaveLength(1);
    expect(await backend.listSpeedAnalysisJobs({ ...filter, offset: 0, limit: 0 })).toEqual({
      jobs: [],
      total: 2,
    });
    expect((await backend.getSpeedAnalysis({ ...filter, space_id: "absent" })).pairs).toHaveLength(0);
  });
});
