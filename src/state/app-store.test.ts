import { afterEach, describe, expect, it, vi } from "vitest";
import { createMockBackend } from "../api/mock/mock-backend";
import { AppStore } from "./app-store";
import { newProject, newSpace } from "./factories";
import type { Backend } from "../api/backend";
import type { WorkspaceStatus } from "../api/types";

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

  it("loads the snapshot, active space and all-project workspace status", async () => {
    await setup();
    expect(store.error).toBeNull();
    expect(store.space).not.toBeNull();
    expect(store.context?.projectId).toBeNull();
    expect(store.status && "context" in store.status && store.status.context.projectId).toBeNull();
    expect(store.status?.flows.length).toBeGreaterThan(0);
    expect(document.documentElement.dataset.theme).toMatch(/^omb-/);
  });

  it("initializes the Active jobs window without loading workspace status or dialogs", async () => {
    const backend = createMockBackend({ seedRunningTransfer: true, tickMs: 5 });
    const getWorkspaceStatus = vi.spyOn(backend, "getWorkspaceStatus");
    store = new AppStore(backend);
    await store.initActiveJobs();

    expect(store.snapshot).not.toBeNull();
    expect(store.transfers.length).toBeGreaterThan(0);
    expect(store.status).toBeNull();
    expect(store.dialogs).toEqual([]);
    expect(getWorkspaceStatus).not.toHaveBeenCalled();
  });

  describe("project-free workspace state", () => {
    let store: AppStore;
    afterEach(() => {
      store?.dispose();
      vi.restoreAllMocks();
    });

    async function backendWithoutProjects(): Promise<Backend> {
      const backend = createMockBackend({ tickMs: 5 });
      const snapshot = await backend.getSnapshot();
      for (const project of snapshot.projects) await backend.deleteEntity("project", project.id);
      const destination = snapshot.destinations.find((d) => d.id === "d2")!;
      await backend.saveEntity("destination", { ...destination, path_template: "Photos", rules: [] });
      return backend;
    }

    it("loads real status and runs available folder transfers without creating a project", async () => {
      const backend = await backendWithoutProjects();
      const statusRequest = vi.spyOn(backend, "getWorkspaceStatus");
      store = new AppStore(backend);
      await store.init();
      expect(store.context).toEqual({ spaceId: "travel", projectId: null });
      expect(statusRequest).toHaveBeenCalledWith(store.context);
      expect(store.status?.sources.length).toBe(3);
      expect(store.statusLoading).toBe(false);
      expect(store.statusError).toBeNull();
      expect(store.status?.sources.every((s) => s.required_copies === 2 && !s.wipe_eligible)).toBe(true);
      const request = {
        context: store.context!,
        flowId: "f4",
        category: "to_transfer" as const,
        offset: 0,
        limit: 10,
      };
      expect((await store.listWorkspaceFiles(request)).items.length).toBeGreaterThan(0);
      await store.runAll();
      await until(() => store.transfers.length > 0 && store.transfers.every((j) => j.state === "done"));
      await store.retryStatus();
      expect(store.status?.flows.find((f) => f.flow_id === "f4")?.state).toBe("done");
      expect((await backend.getSnapshot()).projects).toEqual([]);
    });

    it("keeps workspace status across project creation and deletion", async () => {
      store = new AppStore(await backendWithoutProjects());
      await store.init();
      const project = newProject(store.space!, "First shoot");
      expect(await store.save("project", project)).toBe(true);
      await store.retryStatus();
      expect(store.status && "context" in store.status && store.status.context.projectId).toBeNull();
      await store.remove("project", project.id);
      await store.retryStatus();
      expect(store.status && "context" in store.status && store.status.context.projectId).toBeNull();
      expect(store.statusLoading).toBe(false);
    });

    it("ends loading on status failure and can retry without restarting", async () => {
      const backend = await backendWithoutProjects();
      const request = vi
        .spyOn(backend, "getWorkspaceStatus")
        .mockRejectedValueOnce(new Error("Catalog unavailable"));
      vi.spyOn(console, "error").mockImplementation(() => {});
      store = new AppStore(backend);
      await store.init();
      expect(store.error).toBeNull();
      expect(store.status).toBeNull();
      expect(store.statusLoading).toBe(false);
      expect(store.statusError).toContain("Catalog unavailable");
      await store.retryStatus();
      expect(request).toHaveBeenCalledTimes(2);
      expect(store.status).not.toBeNull();
      expect(store.statusError).toBeNull();
    });

    it("ignores status responses from the previous workspace", async () => {
      const backend = await backendWithoutProjects();
      const original = backend.getWorkspaceStatus;
      let finishOld!: (status: WorkspaceStatus) => void;
      const old = await original({ spaceId: "travel", projectId: null });
      const request = vi.spyOn(backend, "getWorkspaceStatus").mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finishOld = resolve;
          }),
      );
      store = new AppStore(backend);
      const init = store.init();
      await until(() => request.mock.calls.length === 1);
      const other = store.snapshot!.spaces.find((s) => s.id !== "travel")!;
      await store.selectSpace(other.id);
      finishOld(old);
      await init;
      expect(store.status && "context" in store.status && store.status.context.spaceId).toBe(other.id);
      expect(store.statusError).toBeNull();
      expect(store.statusLoading).toBe(false);
    });

    it("ignores failures from a request invalidated by a context switch", async () => {
      const backend = await backendWithoutProjects();
      store = new AppStore(backend);
      await store.init();
      let rejectOld!: (error: Error) => void;
      vi.spyOn(backend, "getWorkspaceStatus").mockImplementationOnce(
        () =>
          new Promise((_resolve, reject) => {
            rejectOld = reject;
          }),
      );
      const retry = store.retryStatus();
      const other = store.snapshot!.spaces.find((s) => s.id !== "travel")!;
      await store.selectSpace(other.id);
      rejectOld(new Error("Old workspace failed"));
      await retry;
      expect(store.status && "context" in store.status && store.status.context.spaceId).toBe(other.id);
      expect(store.statusError).toBeNull();
      expect(store.toasts.some((t) => t.message.includes("Old workspace failed"))).toBe(false);
    });

    it("does not strand an in-flight status request when unrelated settings change", async () => {
      const backend = await backendWithoutProjects();
      store = new AppStore(backend);
      await store.init();
      const status = await backend.getWorkspaceStatus(store.context!);
      let finish!: (value: WorkspaceStatus) => void;
      vi.spyOn(backend, "getWorkspaceStatus").mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      );
      const pending = store.retryStatus();
      await store.saveSettings({ theme: "light" });
      finish(status);
      await pending;
      expect(store.statusLoading).toBe(false);
      expect(store.status).toEqual(status);
    });

    it("coalesces refresh events without discarding a completed status result", async () => {
      const backend = await backendWithoutProjects();
      store = new AppStore(backend);
      await store.init();
      const status = await backend.getWorkspaceStatus(store.context!);
      let finish!: (value: WorkspaceStatus) => void;
      const request = vi.spyOn(backend, "getWorkspaceStatus").mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      );
      const pending = store.retryStatus();
      for (let n = 0; n < 4; n++) {
        store.refreshStatus();
        await new Promise((resolve) => setTimeout(resolve, 130));
      }
      expect(request).toHaveBeenCalledTimes(1);
      finish(status);
      await pending;
      expect(store.status).toEqual(status);
      expect(store.statusLoading).toBe(false);
      await until(() => request.mock.calls.length === 2);
      expect(request).toHaveBeenCalledTimes(2);
    });

    it("opens and confirms manual imports without a project", async () => {
      const backend = await backendWithoutProjects();
      store = new AppStore(backend);
      await store.init();
      const prepare = vi.spyOn(backend, "openWorkspaceFlowInApp");
      const confirm = vi.spyOn(backend, "confirmWorkspaceAppImport");
      await store.openFlowInApp("f7");
      expect(prepare).toHaveBeenCalledWith({ spaceId: "travel", projectId: null }, "f7");
      const dialog = store.dialogs.at(-1);
      expect(dialog?.type).toBe("confirm");
      if (dialog?.type !== "confirm") throw new Error("Missing import confirmation");
      await dialog.onConfirm();
      expect(confirm).toHaveBeenCalledWith({ spaceId: "travel", projectId: null }, "f7", expect.any(String));
      expect(store.transfers.some((job) => job.kind === "app_import" && job.state === "queued")).toBe(true);
      expect(store.toasts.at(-1)?.kind).toBe("info");
      await until(
        () => store.transfers.some((job) => job.kind === "app_import" && job.state === "done"),
        8000,
      );
      expect(store.toasts.at(-1)?.kind).toBe("success");
      await store.retryStatus();
      expect(store.status?.destinations.find((d) => d.destination_id === "d4")?.transferred).toBeGreaterThan(
        0,
      );
      expect(store.status?.sources.every((s) => !s.wipe_eligible)).toBe(true);
    });
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

  it.each(["done", "failed", "cancelled"] as const)(
    "reports a background import %s result once without opening a dialog",
    async (state) => {
      const backend = createMockBackend();
      const on = vi.spyOn(backend, "on");
      store = new AppStore(backend);
      await store.init();
      const receive = on.mock.calls.find(([event]) => event === "transfers")![1];
      const job = {
        id: "mark-result",
        flow_id: "app-import:f7",
        label: "Mark as transferred",
        kind: "app_import" as const,
        state,
        files_done: 2,
        files_total: 3,
        bytes_done: 20,
        bytes_total: 30,
        current_file: null,
        speed_bps: 0,
        bytes_per_sec: null,
        eta_secs: null,
        errors: state === "failed" ? ["Source disconnected"] : [],
      };
      receive([job]);
      const result = store.toasts.at(-1);
      expect(result?.kind).toBe(state === "failed" ? "error" : state === "cancelled" ? "warning" : "success");
      expect(result?.message).toContain("2 files marked transferred");
      if (state === "failed") expect(result?.message).toContain("Source disconnected");
      expect(store.dialogs).toHaveLength(0);
      const count = store.toasts.length;
      receive([job]);
      expect(store.toasts).toHaveLength(count);
    },
  );

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

  it.each(["init", "initActiveJobs"] as const)(
    "shows native sleep-prevention warnings without changing transfer results in %s",
    async (init) => {
      const backend = createMockBackend();
      const on = vi.spyOn(backend, "on");
      store = new AppStore(backend);
      await store[init]();
      const before = structuredClone(store.transfers);
      const message = "Could not keep this computer awake. Transfers will continue.";
      const subscription = on.mock.calls.find(([event]) => event === "transfer-power-warning");
      expect(subscription).toBeDefined();
      subscription![1](message);
      expect(store.toasts.at(-1)).toMatchObject({ kind: "warning", message });
      expect(store.transfers).toEqual(before);
      expect(store.error).toBeNull();
    },
  );

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
