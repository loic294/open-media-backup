import { describe, expect, it } from "vitest";
import { createMockBackend } from "./mock-backend";

describe("mock backup naming", () => {
  it("persists independent names and changes paths only for backup overrides", async () => {
    const backend = createMockBackend();
    const snapshot = await backend.getSnapshot();
    const source = snapshot.sources[0];
    const destination = snapshot.destinations[0];
    destination.path_template = "{source_name}";
    await backend.saveEntity("destination", destination);
    const request = {
      context: { spaceId: source.space_id, projectId: "trip" },
      flowId: "f1",
      category: "transferred" as const,
      offset: 0,
      limit: 1,
    };
    const before = await backend.listWorkspaceFiles(request);
    source.task_name = "Photo ingest";
    destination.task_name = "RAW backup";
    await backend.saveEntity("source", source);
    await backend.saveEntity("destination", destination);
    expect(await backend.listWorkspaceFiles(request)).toEqual(before);
    source.backup_name = " Camera A ";
    await backend.saveEntity("source", source);
    const after = await backend.listWorkspaceFiles(request);
    expect(after.items[0].target_path).toBe(`Camera A/Camera A/${after.items[0].rel_path}`);
    expect(
      await backend.listFiles({
        projectId: "trip",
        flowId: "f1",
        category: "transferred",
        offset: 0,
        limit: 1,
      }),
    ).toEqual(after);
    const reloaded = await backend.getSnapshot();
    expect(reloaded.sources[0]).toMatchObject({ task_name: "Photo ingest", backup_name: " Camera A " });
    expect(reloaded.destinations[0].task_name).toBe("RAW backup");
    expect(reloaded.devices).toEqual(snapshot.devices);
    expect(reloaded.mappings).toEqual(snapshot.mappings);
    expect(reloaded.sources[1]).toEqual(snapshot.sources[1]);
  });

  it("uses a per-source override in condition-rule variables", async () => {
    const backend = createMockBackend();
    const snapshot = await backend.getSnapshot();
    snapshot.sources[0].backup_name = "Camera A";
    const destination = snapshot.destinations[0];
    destination.rules = [{ kind: "condition", expr: { op: "eq", var: "source_name", value: "Camera A" } }];
    await backend.saveEntity("source", snapshot.sources[0]);
    await backend.saveEntity("destination", destination);
    const request = {
      context: { spaceId: "travel", projectId: "trip" },
      flowId: "f1",
      category: "transferred" as const,
      offset: 0,
      limit: 1,
    };
    expect((await backend.listWorkspaceFiles(request)).items).toHaveLength(1);
    snapshot.sources[0].backup_name = "Camera B";
    await backend.saveEntity("source", snapshot.sources[0]);
    expect((await backend.listWorkspaceFiles(request)).items).toHaveLength(0);
  });
});
