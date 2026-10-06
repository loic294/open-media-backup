import { describe, expect, it } from "vitest";
import { demoCounts, demoSnapshot } from "./data";
import { manuallyWipedCounts } from "./manual-wipe";
import { createMockBackend } from "./mock-backend";
import type { Counts } from "./status";

describe("manual wipe mock transition", () => {
  it("retires all source flows on the device across spaces and preserves unrelated counts", () => {
    const snapshot = demoSnapshot();
    const source = snapshot.sources.find((item) => item.id === "s1")!;
    snapshot.sources.push({ ...source, id: "sibling", space_id: "other" });
    snapshot.flows.push({
      ...snapshot.flows[0],
      id: "sibling-flow",
      source_id: "sibling",
      space_id: "other",
    });
    const counts: Counts = {
      ...structuredClone(demoCounts),
      "sibling-flow": [1, 2, 3, 4] as [number, number, number, number],
    };
    const next = manuallyWipedCounts(snapshot, counts, source.device_id);
    for (const flow of snapshot.flows.filter((item) => ["s1", "sibling"].includes(item.source_id)))
      expect(next[flow.id]).toEqual([0, 0, 0, 0]);
    expect(next.f4).toEqual(counts.f4);
    expect(counts["sibling-flow"]).toEqual([1, 2, 3, 4]);
    expect(manuallyWipedCounts(snapshot, next, source.device_id)).toEqual(next);
  });

  it("reopens copy claims targeting a wiped temporary device without retiring the other source", () => {
    const snapshot = demoSnapshot();
    const flow = snapshot.flows.find((item) => item.id === "f1")!;
    const destination = snapshot.destinations.find((item) => item.id === flow.destination_id)!;
    const next = manuallyWipedCounts(snapshot, structuredClone(demoCounts), destination.device_id);
    expect(next.f1).toEqual([0, 1248, 0, 0]);
  });

  it("updates all project and workspace views and file lists, including an offline source", async () => {
    const backend = createMockBackend();
    const snapshot = await backend.getSnapshot();
    const source = snapshot.sources.find((item) => item.id === "s1")!;
    const sibling = { ...source, id: "sibling", space_id: "home" };
    await backend.saveEntity("source", sibling);
    const flow = snapshot.flows.find((item) => item.source_id === "s1")!;
    await backend.saveEntity("flow", {
      ...flow,
      id: "sibling-flow",
      source_id: sibling.id,
      space_id: "home",
    });
    await backend.markSourceManuallyWiped("s1");
    const status = await backend.getWorkspaceStatus({ spaceId: source.space_id, projectId: null });
    expect(status.sources.find((item) => item.source_id === source.id)).toMatchObject({
      file_count: 0,
      safe_copies: 0,
      wipe_eligible: false,
    });
    for (const project of snapshot.projects.filter((item) => item.space_id === source.space_id)) {
      const projectStatus = await backend.getProjectStatus(project.id);
      expect(projectStatus.sources.find((item) => item.source_id === source.id)?.safe_copies).toBe(0);
    }
    const page = await backend.listWorkspaceFiles({
      context: { spaceId: source.space_id, projectId: null },
      flowId: flow.id,
      category: "transferred",
      offset: 0,
      limit: 100,
    });
    expect(page.total).toBe(0);
    const home = await backend.getWorkspaceStatus({ spaceId: "home", projectId: null });
    expect(home.flows.find((item) => item.flow_id === "sibling-flow")?.transferred).toBe(0);
    await backend.saveEntity("device", {
      ...snapshot.devices.find((item) => item.id === "hdd")!,
      role: "temporary",
    });
    await backend.saveEntity("source", { ...source, device_id: "hdd" });
    await expect(backend.markSourceManuallyWiped(source.id)).resolves.toBeUndefined();
  });

  it("rejects active jobs, missing devices and final devices without changing state", async () => {
    const backend = createMockBackend({ seedRunningTransfer: true });
    const snapshot = await backend.getSnapshot();
    const jobs = await backend.listTransfers();
    const flow = snapshot.flows.find((item) => item.id === jobs[0].flow_id)!;
    await expect(backend.markSourceManuallyWiped(flow.source_id)).rejects.toThrow("active jobs");
    for (const job of jobs) await backend.cancelTransfer(job.id);
    await expect(backend.markSourceManuallyWiped("unknown")).rejects.toThrow("source not found");
    const source = snapshot.sources[0];
    await backend.saveEntity("source", { ...source, device_id: "" });
    await expect(backend.markSourceManuallyWiped(source.id)).rejects.toThrow("Select a device");
    await backend.saveEntity("source", {
      ...source,
      device_id: snapshot.devices.find((item) => item.role === "final")!.id,
    });
    await expect(backend.markSourceManuallyWiped(source.id)).rejects.toThrow("Final");
  });

  it("blocks pending app imports so their old confirmations cannot repopulate the wiped catalog", async () => {
    const backend = createMockBackend();
    const snapshot = await backend.getSnapshot();
    const flow = snapshot.flows.find((item) =>
      snapshot.destinations.some(
        (destination) => destination.id === item.destination_id && destination.kind === "app",
      ),
    )!;
    const context = { spaceId: flow.space_id, projectId: null };
    const opened = await backend.openWorkspaceFlowInApp(context, flow.id);
    await expect(backend.markSourceManuallyWiped(flow.source_id)).rejects.toThrow("pending app imports");
    await backend.confirmWorkspaceAppImport(context, flow.id, opened.token);
    await backend.markSourceManuallyWiped(flow.source_id);
    const status = await backend.getWorkspaceStatus(context);
    expect(status.sources.find((source) => source.source_id === flow.source_id)?.safe_copies).toBe(0);
  });
});
