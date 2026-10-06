import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { tauriBackend } from "./tauri-backend";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("destination job command adapters", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });
  const context = { spaceId: "space", projectId: null };

  it("uses hash-server commands with ids and server-relative paths only", async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await tauriBackend.listHashServers();
    expect(invoke).toHaveBeenLastCalledWith("list_hash_servers");
    await tauriBackend.addHashServer("nas:47822", "secret");
    expect(invoke).toHaveBeenLastCalledWith("add_hash_server", { address: "nas:47822", token: "secret" });
    await tauriBackend.hashServerRoots("server");
    expect(invoke).toHaveBeenLastCalledWith("hash_server_roots", { id: "server" });
    await tauriBackend.hashServerBrowse("server", "root", "Archive");
    expect(invoke).toHaveBeenLastCalledWith("hash_server_browse", {
      id: "server",
      root: "root",
      path: "Archive",
    });
    await tauriBackend.testRemoteHashMapping("destination");
    expect(invoke).toHaveBeenLastCalledWith("test_remote_hash_mapping", { destinationId: "destination" });
    await tauriBackend.removeHashServer("server");
    expect(invoke).toHaveBeenLastCalledWith("remove_hash_server", { id: "server" });
    vi.mocked(invoke).mockRejectedValue(new Error("Unauthorized"));
    await expect(tauriBackend.hashServerRoots("server")).rejects.toThrow("Unauthorized");
  });
  it("requests safe-copy evidence and persists rules by stored source id only", async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await tauriBackend.getSourceSafeCopyDetails(context, "source");
    expect(invoke).toHaveBeenLastCalledWith("get_source_safe_copy_details", { context, sourceId: "source" });
    const rules = [{ action: "exclude" as const, syntax: "glob" as const, pattern: "*.THM" }];
    await tauriBackend.saveSourceSafeCopyRules(context, "source", rules);
    expect(invoke).toHaveBeenLastCalledWith("save_source_safe_copy_rules", {
      context,
      sourceId: "source",
      rules,
    });
    vi.mocked(invoke).mockRejectedValue(new Error("Device is busy"));
    await expect(tauriBackend.saveSourceSafeCopyRules(context, "source", rules)).rejects.toThrow(
      "Device is busy",
    );
  });

  it("marks manual wipe by stored source id only and propagates persistence failure", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await tauriBackend.markSourceManuallyWiped("source");
    expect(invoke).toHaveBeenCalledWith("mark_source_manually_wiped", { sourceId: "source" });
    vi.mocked(invoke).mockRejectedValue(new Error("catalog write failed"));
    await expect(tauriBackend.markSourceManuallyWiped("source")).rejects.toThrow("catalog write failed");
  });

  it("queues destination Run and Check using the same project-free workspace context", async () => {
    vi.mocked(invoke).mockResolvedValue(["job-a", "job-b"]);
    expect(await tauriBackend.runWorkspaceDestination(context, "destination")).toEqual(["job-a", "job-b"]);
    expect(invoke).toHaveBeenLastCalledWith("run_workspace_destination", {
      context,
      destinationId: "destination",
    });
    expect(await tauriBackend.checkWorkspaceDestination(context, "destination")).toEqual(["job-a", "job-b"]);
    expect(invoke).toHaveBeenLastCalledWith("check_workspace_destination", {
      context,
      destinationId: "destination",
      scope: { kind: "configuredSources" },
    });
  });

  it("passes selected source ids and full destination scope intact", async () => {
    vi.mocked(invoke).mockResolvedValue(["job"]);
    for (const scope of [
      { kind: "selectedSources" as const, sourceIds: ["source"] },
      { kind: "allDestination" as const },
    ]) {
      await tauriBackend.checkWorkspaceDestination(context, "destination", scope);
      expect(invoke).toHaveBeenLastCalledWith("check_workspace_destination", {
        context,
        destinationId: "destination",
        scope,
      });
    }
  });
  it("passes the live request and queue-only apply-all decision to Rust", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await tauriBackend.resolveTransferConflict("job", "request", "keep_both", true);
    expect(invoke).toHaveBeenCalledWith("resolve_transfer_conflict", {
      jobId: "job",
      requestId: "request",
      decision: "keep_both",
      applyToRemaining: true,
    });

    vi.mocked(invoke).mockRejectedValue(new Error("request expired"));
    await expect(tauriBackend.resolveTransferConflict("job", "request", "replace", false)).rejects.toThrow(
      "request expired",
    );
  });

  it("passes analysis filters and pagination intact and propagates query failures", async () => {
    const req = { space_id: "space", since: 1000, pair_id: "pair" };
    vi.mocked(invoke).mockResolvedValue({});
    await tauriBackend.getSpeedAnalysis(req);
    expect(invoke).toHaveBeenLastCalledWith("get_speed_analysis", { req });
    const page = { ...req, offset: 5, limit: 25 };
    await tauriBackend.listSpeedAnalysisJobs(page);
    expect(invoke).toHaveBeenLastCalledWith("list_speed_analysis_jobs", { req: page });
    vi.mocked(invoke).mockRejectedValue(new Error("history unavailable"));
    await expect(tauriBackend.getSpeedAnalysis(req)).rejects.toThrow("history unavailable");
  });
});
