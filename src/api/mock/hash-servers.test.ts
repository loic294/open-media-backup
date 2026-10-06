import { describe, expect, it } from "vitest";
import { createMockBackend } from "./mock-backend";
import { DEMO_HASH_ROOT_ID, DEMO_HASH_SERVER_ID } from "./hash-servers";

describe("local hash-server mock API", () => {
  it("pairs, lists and browses without exposing tokens in snapshot or status", async () => {
    const backend = createMockBackend();
    const before = await backend.getSnapshot();
    await expect(backend.addHashServer("nas:47822", "secret")).rejects.toThrow("demo-token");
    expect(await backend.listHashServers()).toEqual([]);
    await backend.addHashServer("nas:47822", "demo-token");
    expect(await backend.listHashServers()).toEqual([expect.objectContaining({ id: DEMO_HASH_SERVER_ID })]);
    expect(JSON.stringify(await backend.listHashServers())).not.toContain("demo-token");
    expect(await backend.getSnapshot()).toEqual(before);
    const roots = await backend.hashServerRoots(DEMO_HASH_SERVER_ID);
    expect(roots[0].path).toBe("/data/photos");
    expect(await backend.hashServerBrowse(DEMO_HASH_SERVER_ID, roots[0].id, "")).toEqual({
      path: "",
      directories: ["Archive"],
    });
    await expect(backend.hashServerBrowse(DEMO_HASH_SERVER_ID, roots[0].id, "../secret")).rejects.toThrow();
    const destination = before.destinations.find((d) => d.id === "d1")!;
    await backend.saveEntity("destination", {
      ...destination,
      remote_hash: {
        server_id: DEMO_HASH_SERVER_ID,
        root: DEMO_HASH_ROOT_ID,
        enabled: true,
      },
    });
    expect((await backend.getSnapshot()).destinations.find((d) => d.id === "d1")?.remote_hash).toEqual({
      server_id: DEMO_HASH_SERVER_ID,
      root: DEMO_HASH_ROOT_ID,
      enabled: true,
    });
    const test = await backend.testRemoteHashMapping("d1");
    expect(test.verified).toBe(false);
    expect(test.message).toContain("No real files were hashed");
    await backend.removeHashServer(DEMO_HASH_SERVER_ID);
    expect(await backend.listHashServers()).toEqual([]);
    await expect(backend.testRemoteHashMapping("d1")).rejects.toThrow("not available on this computer");
    expect((await backend.getSnapshot()).destinations.find((d) => d.id === "d1")?.remote_hash?.enabled).toBe(
      true,
    );
  });
});
