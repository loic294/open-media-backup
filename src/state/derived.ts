import type {
  Destination,
  DestinationStatus,
  Flow,
  FlowStatus,
  ProjectStatus,
  TransferJob,
} from "../api/types";

/** Used for ETA estimates before a transfer has measured its real speed. */
export const ASSUMED_SPEED_BPS = 120_000_000;

export interface ProjectTotals {
  flows: number;
  pending: number;
  errors: number;
  runnable: number;
}

export function projectTotals(
  status: ProjectStatus | null,
  flows: Flow[],
  destinations: Destination[] = [],
): ProjectTotals {
  const all = status?.flows ?? [];
  if (!destinations.length) {
    return {
      flows: flows.length,
      pending: all.reduce((n, f) => n + f.to_transfer, 0),
      errors: all.reduce((n, f) => n + f.failed, 0),
      runnable: all.filter(isRunnable).reduce((n, f) => n + f.to_transfer + f.failed, 0),
    };
  }
  const appDestinations = new Set(
    destinations.filter((d) => (d.kind ?? "folder") === "app").map((d) => d.id),
  );
  const autoFlowIds = new Set(flows.filter((f) => !appDestinations.has(f.destination_id)).map((f) => f.id));
  const automatic = all.filter((f) => autoFlowIds.has(f.flow_id));
  return {
    flows: flows.length,
    pending: automatic.reduce((n, f) => n + f.to_transfer, 0),
    errors: automatic.reduce((n, f) => n + f.failed, 0),
    runnable: automatic.filter(isRunnable).reduce((n, f) => n + f.to_transfer + f.failed, 0),
  };
}

export function isRunnable(f: FlowStatus): boolean {
  return (f.state === "pending" || f.state === "error") && f.to_transfer + f.failed > 0;
}

export function flowStatus(status: ProjectStatus | null, flowId: string): FlowStatus | undefined {
  return status?.flows.find((f) => f.flow_id === flowId);
}

export function destinationStatus(status: ProjectStatus | null, id: string): DestinationStatus | undefined {
  return status?.destinations.find((d) => d.destination_id === id);
}

export function sourceStatus(status: ProjectStatus | null, id: string) {
  return status?.sources.find((s) => s.source_id === id);
}

export function isActive(job: TransferJob): boolean {
  return (
    job.state === "queued" || job.state === "running" || job.state === "verifying" || job.state === "paused"
  );
}

export interface TransferTotals {
  active: TransferJob[];
  running: number;
  paused: boolean;
  bytesDone: number;
  bytesTotal: number;
  etaSeconds: number | null;
}

export function transferTotals(jobs: TransferJob[]): TransferTotals {
  const active = jobs.filter(isActive);
  const bytesDone = active.reduce((n, j) => n + j.bytes_done, 0);
  const bytesTotal = active.reduce((n, j) => n + j.bytes_total, 0);
  const speed = active.reduce((n, j) => n + (j.state === "paused" ? 0 : j.speed_bps), 0);
  return {
    active,
    running: active.filter((j) => j.state !== "paused").length,
    paused: active.length > 0 && active.every((j) => j.state === "paused"),
    bytesDone,
    bytesTotal,
    etaSeconds: speed > 0 ? (bytesTotal - bytesDone) / speed : null,
  };
}

/** Human label for a flow's failure, e.g. "3 hash mismatches". */
export function failureLabel(failed: number, error: string | null): string {
  if (error?.toLowerCase().includes("hash"))
    return `${failed.toLocaleString("en-US")} hash ${failed === 1 ? "mismatch" : "mismatches"}`;
  return `${failed.toLocaleString("en-US")} ${failed === 1 ? "error" : "errors"}`;
}
