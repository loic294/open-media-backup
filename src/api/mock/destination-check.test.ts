import { afterEach, describe, expect, it } from "vitest";
import type { Backend } from "../backend";
import { createMockBackend } from "./mock-backend";

async function until(
  backend: Backend,
  ready: (jobs: Awaited<ReturnType<Backend["listTransfers"]>>) => boolean,
) {
  for (let attempt = 0; attempt < 300; attempt++) {
    const jobs = await backend.listTransfers();
    if (ready(jobs)) return jobs;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  throw new Error("Jobs did not reach the expected state");
}

describe("mock destination jobs", () => {
  let backend: Backend;
  const context = { spaceId: "travel", projectId: "trip" };
  afterEach(async () => {
    if (!backend) return;
    for (const job of await backend.listTransfers()) {
      if (!["done", "failed", "cancelled"].includes(job.state)) await backend.cancelTransfer(job.id);
    }
  });

  it("checks existing and missing paths without transferring or learning speeds", async () => {
    backend = createMockBackend({ tickMs: 2 });
    const before = await backend.getProjectStatus("trip");
    const snapshot = await backend.getSnapshot();
    const ids = await backend.checkWorkspaceDestination(context, "d2");
    expect(ids).toHaveLength(2);
    const jobs = await until(backend, (jobs) => jobs.every((job) => job.state === "done"));
    expect(jobs.every((job) => job.kind === "check")).toBe(true);
    expect(jobs.find((job) => job.flow_id === "check:f2")?.check_results?.matched).toBeGreaterThan(0);
    expect(jobs.find((job) => job.flow_id === "check:f4")?.check_results?.missing).toBeGreaterThan(0);
    expect(
      jobs.flatMap((job) => job.check_results?.items ?? []).some((item) => item.outcome === "matched"),
    ).toBe(false);
    expect(await backend.getProjectStatus("trip")).toEqual(before);
    expect((await backend.getSnapshot()).settings.transfer_speeds).toEqual(snapshot.settings.transfer_speeds);
    await expect(backend.checkWorkspaceDestination(context, "d4")).rejects.toThrow("folder destination");
  });

  it("filters selected sources and rejects invalid selections before enqueueing", async () => {
    backend = createMockBackend({ tickMs: 2 });
    const snapshot = await backend.getSnapshot();
    const source = snapshot.flows.find((f) => f.id === "f4")!.source_id;
    for (const sourceIds of [[], [source, "stale"], [source, "foreign"]]) {
      await expect(
        backend.checkWorkspaceDestination(context, "d2", {
          kind: "selectedSources",
          sourceIds,
        }),
      ).rejects.toThrow();
      expect(await backend.listTransfers()).toHaveLength(0);
    }
    const ids = await backend.checkWorkspaceDestination(context, "d2", {
      kind: "selectedSources",
      sourceIds: [source, source],
    });
    expect(ids).toHaveLength(1);
    expect((await backend.listTransfers())[0].flow_id).toBe("check:f4");
  });

  it("reports a distinct destination inventory with catalog verification and untracked files", async () => {
    backend = createMockBackend({ tickMs: 2 });
    const ids = await backend.checkWorkspaceDestination(context, "d2", { kind: "allDestination" });
    expect(ids).toHaveLength(1);
    const [job] = await until(backend, (jobs) => jobs.every((job) => job.state === "done"));
    expect(job.flow_id).toBe("check:destination:d2");
    expect(job.check_results).toMatchObject({
      matched: 0,
      verified: 1,
      untracked: 1,
      missing: 1,
      conflicts: 1,
      errors: 1,
    });
    expect(job.check_results!.items.every((item) => !item.source_path)).toBe(true);
    expect(job.analysis).toBeUndefined();
  });

  it("applies Skip to this queue's pending and future conflicts but prompts on the next run", async () => {
    backend = createMockBackend({ tickMs: 2, conflicts: { f4: 2, f5: 2 } });
    const initialSpeeds = (await backend.getSnapshot()).settings.transfer_speeds;
    await backend.runAll("trip");
    const waiting = await until(backend, (jobs) => jobs.filter((job) => job.pending_conflict).length === 2);
    const first = waiting.find((job) => job.pending_conflict)!;
    await backend.resolveTransferConflict(first.id, first.pending_conflict!.request_id, "skip", true);
    await expect(
      backend.resolveTransferConflict(first.id, first.pending_conflict!.request_id, "replace", false),
    ).rejects.toThrow("no longer pending");
    await until(backend, (jobs) => jobs.every((job) => job.state === "done"));
    const status = await backend.getProjectStatus("trip");
    expect(status.flows.find((flow) => flow.flow_id === "f4")?.to_transfer).toBe(2);
    expect(status.flows.find((flow) => flow.flow_id === "f5")?.to_transfer).toBe(2);
    expect((await backend.getSnapshot()).settings.transfer_speeds).toEqual(initialSpeeds);
    await backend.runAll("trip");
    const next = await until(backend, (jobs) => jobs.some((job) => job.pending_conflict));
    expect(next.some((job) => job.pending_conflict && job.id !== first.id)).toBe(true);
  });

  it("prompts separately unless apply-all is selected and cancels without a stale request", async () => {
    backend = createMockBackend({ tickMs: 2, conflicts: { f4: 2 } });
    await backend.runWorkspaceDestination(context, "d2");
    const [first] = await until(backend, (jobs) => jobs.some((job) => job.pending_conflict));
    await backend.resolveTransferConflict(first.id, first.pending_conflict!.request_id, "keep_both", false);
    const [second] = await until(backend, (jobs) =>
      jobs.some(
        (job) =>
          job.pending_conflict && job.pending_conflict.request_id !== first.pending_conflict!.request_id,
      ),
    );
    await backend.cancelTransfer(second.id);
    expect((await backend.listTransfers())[0].pending_conflict).toBeNull();
    await expect(
      backend.resolveTransferConflict(second.id, second.pending_conflict!.request_id, "replace", false),
    ).rejects.toThrow("no longer pending");
  });
});
