import type {
  AnalysisFilter,
  AnalysisJob,
  AnalysisJobPage,
  AnalysisJobsRequest,
  AnalysisMetrics,
  AnalysisPhase,
  AnalysisSummary,
  Snapshot,
  TransferJob,
} from "../types";
import { analysisMatches, emptyAnalysisMetrics, summarizeAnalysis } from "../../utils/speed-analysis";
import { destinationTaskName, sourceTaskName } from "../../utils/names";

const PHASE_TIME: Record<Exclude<AnalysisPhase, "finished">, keyof AnalysisMetrics> = {
  queued: "queued_secs",
  other: "other_secs",
  copy: "copy_secs",
  source_check: "source_check_secs",
  destination_check: "destination_check_secs",
  paused: "paused_secs",
  awaiting_decision: "decision_secs",
};

/** Demo-only telemetry. Real reads/writes and durable recording live in the Rust backend. */
export class MockAnalysis {
  #records = new Map<string, AnalysisJob>();
  #last = new Map<string, number>();

  constructor(
    private snapshot: Snapshot,
    history: AnalysisJob[] = [],
  ) {
    for (const job of history) this.#records.set(job.id, structuredClone(job));
  }

  begin(job: TransferJob, flowId: string): void {
    const flow = this.snapshot.flows.find((f) => f.id === flowId);
    const source = this.snapshot.sources.find((s) => s.id === flow?.source_id);
    const destination = this.snapshot.destinations.find((d) => d.id === flow?.destination_id);
    const space = this.snapshot.spaces.find((s) => s.id === flow?.space_id);
    const srcDevice = this.snapshot.devices.find((d) => d.id === source?.device_id);
    const dstDevice = this.snapshot.devices.find((d) => d.id === destination?.device_id);
    if (!flow || !source || !destination || !space || !srcDevice || !dstDevice)
      throw new Error("Cannot record analysis without both devices and a configured flow");
    if (job.kind === "wipe") return;
    const now = Date.now();
    job.analysis = {
      id: job.id,
      context: {
        pair_id: JSON.stringify([space.id, source.id, destination.id, srcDevice.id, dstDevice.id]),
        space_id: space.id,
        space_name: space.name,
        source_id: source.id,
        destination_id: destination.id,
        source_device_id: srcDevice.id,
        destination_device_id: dstDevice.id,
        source_name: sourceTaskName(source, srcDevice),
        destination_name: destinationTaskName(destination, dstDevice),
        source_device_name: srcDevice.name,
        destination_device_name: dstDevice.name,
        hash_algo: space.hash_algo,
        verify_mode: space.verify_mode,
      },
      kind: job.kind === "check" ? "check" : "transfer",
      state: job.state,
      phase: "queued",
      created_at: now,
      updated_at: now,
      finished_at: null,
      metrics: emptyAnalysisMetrics(),
      error_count: 0,
    };
    this.#records.set(job.id, job.analysis);
    this.#last.set(job.id, now);
  }

  observe(job: TransferJob): { phase: AnalysisPhase; seconds: number } | null {
    const record = job.analysis;
    if (!record || record.finished_at !== null) return null;
    const now = Date.now();
    const elapsed = {
      phase: record.phase,
      seconds: Math.max(0, now - (this.#last.get(job.id) ?? now)) / 1000,
    };
    if (record.phase !== "finished") {
      record.metrics[PHASE_TIME[record.phase]] += elapsed.seconds;
    }
    this.#last.set(job.id, now);
    record.updated_at = now;
    record.state = job.state;
    record.error_count = job.errors.length + (job.check_results?.errors ?? 0);
    if (["done", "failed", "cancelled"].includes(job.state)) {
      record.phase = "finished";
      record.finished_at = now;
      this.#last.delete(job.id);
    } else {
      record.phase =
        job.state === "queued" || job.state === "paused" || job.state === "awaiting_decision"
          ? job.state
          : record.kind === "check"
            ? "source_check"
            : "copy";
    }
    return elapsed;
  }

  recordWork(
    job: TransferJob,
    copied: number,
    sourceRead: number,
    destinationRead: number,
    files: number,
  ): void {
    const elapsed = this.observe(job);
    const record = job.analysis;
    if (!record) return;
    const m = record.metrics;
    m.copy_bytes += copied;
    m.committed_bytes += copied;
    m.source_check_bytes += sourceRead;
    m.destination_check_bytes += destinationRead;
    if (record.kind === "transfer") m.transferred_files += files;
    // Model sequential source/destination reads within the demo's single tick.
    if (record.kind === "check" && elapsed?.phase === "source_check") {
      const total = sourceRead + destinationRead;
      m.source_check_secs -= elapsed.seconds;
      if (total > 0) {
        m.source_check_secs += (elapsed.seconds * sourceRead) / total;
        m.destination_check_secs += (elapsed.seconds * destinationRead) / total;
      } else m.other_secs += elapsed.seconds;
    } else if (destinationRead > 0 && elapsed?.phase === "copy") {
      m.copy_secs -= elapsed.seconds * 0.25;
      m.destination_check_secs += elapsed.seconds * 0.25;
    }
  }

  summary(filter: AnalysisFilter): AnalysisSummary {
    return structuredClone(summarizeAnalysis([...this.#records.values()], filter));
  }

  list(req: AnalysisJobsRequest): AnalysisJobPage {
    if (
      !Number.isInteger(req.offset) ||
      req.offset < 0 ||
      !Number.isInteger(req.limit) ||
      req.limit < 0 ||
      req.limit > 100
    )
      throw new Error("Analysis pagination requires offset >= 0 and limit between 0 and 100");
    const jobs = [...this.#records.values()]
      .filter((job) => job.finished_at !== null && analysisMatches(job, req))
      .sort((a, b) => b.created_at - a.created_at || b.id.localeCompare(a.id));
    return { jobs: structuredClone(jobs.slice(req.offset, req.offset + req.limit)), total: jobs.length };
  }
}
