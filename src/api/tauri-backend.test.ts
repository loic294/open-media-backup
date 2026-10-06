import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { tauriBackend } from "./tauri-backend";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("destination job command adapters", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });
  const context = { spaceId: "space", projectId: null };

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
    });
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
