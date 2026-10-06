import { afterEach, describe, expect, it, vi } from "vitest";
import { createMockBackend } from "../api/mock/mock-backend";
import { Emitter } from "../api/mock/emitter";
import type { TransferJob } from "../api/types";
import { AppStore } from "./app-store";

const job = (patch: Partial<TransferJob> = {}): TransferJob => ({
  id: "job",
  flow_id: "f4",
  label: "Transfer",
  state: "awaiting_decision",
  files_done: 0,
  files_total: 1,
  bytes_done: 0,
  bytes_total: 10,
  current_file: "photo.jpg",
  speed_bps: 0,
  bytes_per_sec: null,
  eta_secs: null,
  errors: [],
  kind: "transfer",
  pending_conflict: {
    request_id: "request",
    source_path: "/source/photo.jpg",
    destination_path: "/dest/photo.jpg",
    source_hash: "source",
    destination_hash: "different",
  },
  ...patch,
});

describe("destination actions and job snapshots", () => {
  let store: AppStore;
  afterEach(() => {
    store?.dispose();
    vi.restoreAllMocks();
  });

  async function setup(jobs: TransferJob[] = []) {
    const backend = createMockBackend();
    const events = new Emitter();
    backend.on = async (event, handler) => events.on(event, handler as never);
    backend.listTransfers = vi.fn(async () => structuredClone(jobs));
    store = new AppStore(backend);
    await store.init();
    return { backend, events };
  }

  it("dispatches destination Run as a single queue and Check with the current context", async () => {
    const { backend } = await setup();
    const run = vi.spyOn(backend, "runWorkspaceDestination").mockResolvedValue(["a", "b"]);
    const check = vi.spyOn(backend, "checkWorkspaceDestination").mockResolvedValue(["check"]);
    const flow = vi.spyOn(backend, "runFlow");
    await store.runDestination("d2");
    await store.checkDestination("d2");
    expect(run).toHaveBeenCalledOnce();
    expect(run).toHaveBeenCalledWith({ spaceId: "travel", projectId: null }, "d2");
    expect(check).toHaveBeenCalledWith(store.context, "d2", { kind: "configuredSources" });
    expect(flow).not.toHaveBeenCalled();
  });

  it("recovers an initial conflict and serializes repeated and concurrent requests", async () => {
    const first = job();
    const second = job({
      id: "job-2",
      pending_conflict: { ...first.pending_conflict!, request_id: "request-2" },
    });
    const { events } = await setup([first, second]);
    expect(store.dialogs).toEqual([{ type: "transfer-conflict", jobId: "job", requestId: "request" }]);
    events.emit("transfers", [first, second]);
    expect(store.dialogs).toHaveLength(1);
    events.emit("transfers", [{ ...first, pending_conflict: null, state: "done" }, second]);
    expect(store.dialogs).toEqual([{ type: "transfer-conflict", jobId: "job-2", requestId: "request-2" }]);
    events.emit("transfers", [{ ...second, pending_conflict: null, state: "cancelled" }]);
    expect(store.dialogs).toHaveLength(0);
  });

  it("uses the exact request identity and preserves the dialog on command failure", async () => {
    const initial = job();
    const { backend } = await setup([initial]);
    vi.spyOn(console, "error").mockImplementation(() => {});
    const resolve = vi
      .spyOn(backend, "resolveTransferConflict")
      .mockRejectedValue(new Error("stale request"));
    expect(await store.resolveTransferConflict("job", "request", "replace", true)).toBe(false);
    expect(resolve).toHaveBeenCalledWith("job", "request", "replace", true);
    expect(store.dialogs[0]?.type).toBe("transfer-conflict");
    expect(store.toasts.at(-1)?.message).toContain("stale request");
    resolve.mockResolvedValue();
    vi.mocked(backend.listTransfers).mockResolvedValue([
      { ...initial, pending_conflict: null, state: "running" },
    ]);
    expect(await store.resolveTransferConflict("job", "request", "keep_both", false)).toBe(true);
    expect(store.dialogs).toHaveLength(0);
  });

  it("shows check completion once, reports partial results, and refreshes status", async () => {
    const { events } = await setup();
    const refresh = vi.spyOn(store, "refreshStatus");
    const check = job({
      id: "check",
      flow_id: "check:f4",
      label: "Check",
      kind: "check",
      state: "cancelled",
      pending_conflict: null,
      check_results: {
        matched: 1,
        missing: 1,
        conflicts: 0,
        errors: 0,
        verified: 0,
        untracked: 0,
        items: [],
      },
    });
    events.emit("transfers", [check]);
    expect(store.dialogs).toEqual([{ type: "destination-check-results", job: check }]);
    expect(refresh).toHaveBeenCalledOnce();
    store.close();
    events.emit("transfers", [check]);
    expect(store.dialogs).toHaveLength(0);
    expect(refresh).toHaveBeenCalledOnce();
  });

  it("does not overwrite a live conflict with an older startup job list", async () => {
    const backend = createMockBackend();
    const events = new Emitter();
    backend.on = async (event, handler) => events.on(event, handler as never);
    let finish!: (jobs: TransferJob[]) => void;
    backend.listTransfers = () =>
      new Promise((resolve) => {
        finish = resolve;
      });
    store = new AppStore(backend);
    const init = store.init();
    await new Promise((resolve) => setTimeout(resolve, 0));
    events.emit("transfers", [job()]);
    finish([]);
    await init;
    expect(store.transfers).toHaveLength(1);
    expect(store.dialogs).toEqual([{ type: "transfer-conflict", jobId: "job", requestId: "request" }]);
  });

  it("keeps conflict decisions above simultaneous check-result dialogs", async () => {
    const { events } = await setup();
    events.emit("transfers", [
      job(),
      job({ id: "check", kind: "check", state: "done", pending_conflict: null }),
    ]);
    expect(store.dialogs.map((dialog) => dialog.type)).toEqual([
      "destination-check-results",
      "transfer-conflict",
    ]);
  });
});
