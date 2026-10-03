import { afterEach, describe, expect, it, vi } from "vitest";
import { createMockBackend } from "../api/mock/mock-backend";
import { AppStore } from "./app-store";
import { newSpace } from "./factories";

const until = async (cond: () => boolean, ms = 3000) => {
  const end = Date.now() + ms;
  while (!cond()) {
    if (Date.now() > end) throw new Error("timeout");
    await new Promise((r) => setTimeout(r, 10));
  }
};

describe("AppStore with the mock backend", () => {
  let store: AppStore;
  afterEach(() => store?.dispose());

  const setup = async () => {
    store = new AppStore(createMockBackend({ tickMs: 5 }));
    await store.init();
    return store;
  };

  it("loads the snapshot, active space/project and status", async () => {
    await setup();
    expect(store.error).toBeNull();
    expect(store.space).not.toBeNull();
    expect(store.project).not.toBeNull();
    expect(store.status?.flows.length).toBeGreaterThan(0);
    expect(document.documentElement.dataset.theme).toMatch(/^omb-/);
  });

  it("saves and removes entities optimistically", async () => {
    await setup();
    const space = newSpace("Studio", 99);
    const change = vi.fn();
    store.addEventListener("change", change);
    const saving = store.save("space", space);
    expect(store.snapshot!.spaces.some((s) => s.id === space.id)).toBe(true);
    await saving;
    expect(change).toHaveBeenCalled();
    await store.remove("space", space.id);
    expect(store.snapshot!.spaces.some((s) => s.id === space.id)).toBe(false);
  });

  it("stacks and closes dialogs", async () => {
    await setup();
    const a = { type: "app-settings" } as const;
    const b = { type: "device-sync" } as const;
    store.open(a);
    store.open(b);
    store.close(a);
    expect(store.dialogs).toEqual([b]);
    store.close();
    expect(store.dialogs).toEqual([]);
  });

  it("toggles source selection", async () => {
    await setup();
    store.select("s1");
    expect(store.selectedSourceId).toBe("s1");
    store.select("s1");
    expect(store.selectedSourceId).toBeNull();
  });

  it("runs all transfers to completion", async () => {
    await setup();
    const appDestinations = new Set(
      store.snapshot!.destinations.filter((d) => (d.kind ?? "folder") === "app").map((d) => d.id),
    );
    const automaticFlowIds = new Set(
      store.snapshot!.flows.filter((f) => !appDestinations.has(f.destination_id)).map((f) => f.id),
    );
    const pending = () =>
      store.status!.flows.reduce(
        (n, f) => n + (automaticFlowIds.has(f.flow_id) && f.state === "pending" ? f.to_transfer : 0),
        0,
      );
    expect(pending()).toBeGreaterThan(0);
    await store.runAll();
    await until(() => store.transfers.length > 0);
    await until(() => store.transfers.every((j) => j.state !== "running" && j.state !== "queued"), 8000);
    await until(() => pending() === 0);
  }, 15000);
});
