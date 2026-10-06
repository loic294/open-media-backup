import { describe, expect, it } from "vitest";
import { AppStore } from "../../state/app-store";
import { newDevice } from "../../state/factories";
import { createMockBackend } from "./mock-backend";

describe("device management backend", () => {
  it("detaches all matching tasks and mappings but preserves connections and other fields", async () => {
    const backend = createMockBackend();
    const before = await backend.getSnapshot();
    const destination = { ...before.destinations[0], id: "shared-destination", device_id: "card1" };
    const source = { ...before.sources[0], id: "shared-source", space_id: before.spaces[1].id };
    await backend.saveEntity("destination", destination);
    await backend.saveEntity("source", source);
    const original = await backend.getSnapshot();
    await backend.deleteEntity("device", "card1");
    const after = await backend.getSnapshot();
    expect(after.devices).toEqual(original.devices.filter((d) => d.id !== "card1"));
    expect(after.mappings).toEqual(original.mappings.filter((m) => m.device_id !== "card1"));
    expect(after.sources).toEqual(
      original.sources.map((s) => (s.device_id === "card1" ? { ...s, device_id: "" } : s)),
    );
    expect(after.destinations).toEqual(
      original.destinations.map((d) => (d.device_id === "card1" ? { ...d, device_id: "" } : d)),
    );
    expect(after.flows).toEqual(original.flows);
    expect(after.projects).toEqual(original.projects);
    expect(
      (await backend.listVolumes()).find((v) => v.mount_path === "/Volumes/CAM_A_01")?.device_id,
    ).toBeNull();
    const status = await backend.getProjectStatus(original.projects[0].id);
    const sourceStatus = status.sources.find((s) => s.source_id === "s1")!;
    expect(sourceStatus).toMatchObject({ available: false, wipe_eligible: false });
    expect(sourceStatus.blocking_reason).toContain("source settings");
    for (const flow of original.flows.filter((f) => f.source_id === "s1")) {
      expect(status.flows.find((f) => f.flow_id === flow.id)).toMatchObject({
        runnable: false,
        state: "unavailable",
      });
    }
    await expect(backend.runFlow(original.projects[0].id, "f1")).rejects.toThrow("source");
    await expect(
      backend.listFiles({
        projectId: original.projects[0].id,
        flowId: "f1",
        category: "to_transfer",
        offset: 0,
        limit: 20,
      }),
    ).rejects.toThrow("source");
    await expect(backend.wipe("s1", "delete_files")).rejects.toThrow("source");
  });

  it("registers offline devices and can locate them later without duplicating devices or mappings", async () => {
    const backend = createMockBackend();
    const device = newDevice("Offline archive", "hdd", "final");
    await backend.saveEntity("device", device);
    expect((await backend.getSnapshot()).mappings.some((m) => m.device_id === device.id)).toBe(false);
    await backend.relinkDevice(device.id, "/Volumes/Untitled");
    await backend.relinkDevice(device.id, "/Volumes/Untitled");
    let snapshot = await backend.getSnapshot();
    expect(snapshot.devices.filter((d) => d.id === device.id)).toHaveLength(1);
    expect(snapshot.mappings.filter((m) => m.device_id === device.id)).toHaveLength(1);
    expect((await backend.listVolumes()).find((v) => v.mount_path === "/Volumes/Untitled")?.device_id).toBe(
      device.id,
    );
    await backend.registerDevice("/Volumes/Untitled", { ...device, name: "Located archive" });
    snapshot = await backend.getSnapshot();
    expect(snapshot.devices.filter((d) => d.id === device.id)).toHaveLength(1);
    expect(snapshot.devices.find((d) => d.id === device.id)?.name).toBe("Located archive");
    await expect(backend.relinkDevice("missing", "/Volumes/Untitled")).rejects.toThrow("not found");
  });

  it("refreshes the store after device removal and reassignment", async () => {
    const backend = createMockBackend();
    const store = new AppStore(backend);
    store.snapshot = await backend.getSnapshot();
    expect(await store.removeDevice("card1")).toBe(true);
    expect(store.snapshot!.sources.find((s) => s.id === "s1")!.device_id).toBe("");
    expect(store.volumes.find((v) => v.mount_path === "/Volumes/CAM_A_01")?.device_id).toBeNull();
    const source = store.snapshot!.sources.find((s) => s.id === "s1")!;
    await backend.saveEntity("source", { ...source, device_id: "card2" });
    const status = await backend.getProjectStatus(store.snapshot!.projects[0].id);
    expect(status.sources.find((s) => s.source_id === "s1")!.available).toBe(true);
    store.dispose();
  });

  it("blocks folder destinations without devices while app destinations remain device-independent", async () => {
    const backend = createMockBackend();
    const snapshot = await backend.getSnapshot();
    const projectId = snapshot.projects[0].id;
    await backend.deleteEntity("device", "ssd");
    let status = await backend.getProjectStatus(projectId);
    for (const destination of snapshot.destinations.filter((d) => d.device_id === "ssd")) {
      expect(status.destinations.find((d) => d.destination_id === destination.id)).toMatchObject({
        available: false,
        last_error: expect.stringContaining("destination settings"),
      });
    }
    const app = {
      ...snapshot.destinations[0],
      id: "test-app",
      kind: "app" as const,
      device_id: "",
      app_name: "Photo editor",
    };
    await backend.saveEntity("destination", app);
    await backend.saveEntity("flow", {
      id: "test-app-flow",
      space_id: app.space_id,
      source_id: "s1",
      destination_id: app.id,
    });
    status = await backend.getProjectStatus(projectId);
    expect(status.destinations.find((d) => d.destination_id === app.id)?.available).toBe(true);
    expect(status.flows.find((f) => f.flow_id === "test-app-flow")?.runnable).toBe(true);
  });
});
