import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { tauriBackend } from "./tauri-backend";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("destination job command adapters", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });
  const context = { spaceId: "space", projectId: null };

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
});
