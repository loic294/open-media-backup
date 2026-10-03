import { describe, expect, it } from "vitest";
import type { FlowStatus, ProjectStatus, TransferJob } from "../api/types";
import { failureLabel, isRunnable, projectTotals, transferTotals } from "./derived";

const flow = (p: Partial<FlowStatus>): FlowStatus =>
  ({
    flow_id: "f",
    state: "pending",
    to_transfer: 0,
    failed: 0,
    transferred: 0,
    ignored: 0,
    ...p,
  }) as FlowStatus;

const job = (p: Partial<TransferJob>): TransferJob => ({
  id: "j",
  flow_id: "f",
  label: "x",
  state: "running",
  files_done: 0,
  files_total: 10,
  bytes_done: 0,
  bytes_total: 100,
  current_file: null,
  speed_bps: 10,
  errors: [],
  ...p,
});

describe("derived", () => {
  it("only pending/error flows with work are runnable", () => {
    expect(isRunnable(flow({ to_transfer: 2 }))).toBe(true);
    expect(isRunnable(flow({ state: "error", failed: 1 }))).toBe(true);
    expect(isRunnable(flow({ state: "unavailable", to_transfer: 5 }))).toBe(false);
    expect(isRunnable(flow({ to_transfer: 0 }))).toBe(false);
  });

  it("sums project totals", () => {
    const status = {
      flows: [
        flow({ to_transfer: 3 }),
        flow({ state: "error", failed: 2 }),
        flow({ state: "unavailable", to_transfer: 7 }),
      ],
    } as ProjectStatus;
    expect(projectTotals(status, [{}, {}] as never)).toEqual({
      flows: 2,
      pending: 10,
      errors: 2,
      runnable: 5,
    });
    expect(projectTotals(null, [])).toEqual({ flows: 0, pending: 0, errors: 0, runnable: 0 });
  });

  it("excludes app destinations from automatic totals", () => {
    const status = {
      flows: [flow({ flow_id: "auto", to_transfer: 3 }), flow({ flow_id: "app", to_transfer: 7 })],
    } as ProjectStatus;
    expect(
      projectTotals(
        status,
        [
          { id: "auto", destination_id: "folder" },
          { id: "app", destination_id: "lightroom" },
        ] as never,
        [{ id: "lightroom", kind: "app" }] as never,
      ),
    ).toEqual({ flows: 2, pending: 3, errors: 0, runnable: 3 });
  });

  it("computes transfer totals and ETA, ignoring finished and paused jobs", () => {
    const t = transferTotals([
      job({ bytes_done: 50 }),
      job({ state: "paused", bytes_done: 0, speed_bps: 99 }),
      job({ state: "done", bytes_done: 100 }),
    ]);
    expect(t.active).toHaveLength(2);
    expect(t.running).toBe(1);
    expect(t.paused).toBe(false);
    expect(t.bytesDone).toBe(50);
    expect(t.bytesTotal).toBe(200);
    expect(t.etaSeconds).toBe(15);
    expect(transferTotals([job({ state: "paused" })]).paused).toBe(true);
    expect(transferTotals([]).etaSeconds).toBeNull();
  });

  it("labels failures", () => {
    expect(failureLabel(3, "Hash mismatch")).toBe("3 hash mismatches");
    expect(failureLabel(1, "hash mismatch")).toBe("1 hash mismatch");
    expect(failureLabel(1200, "disk full")).toBe("1,200 errors");
  });
});
