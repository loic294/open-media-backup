import { describe, expect, it } from "vitest";
import type { FlowStatus, PeerStatus, SyncStatus } from "../api/types";
import { variableErrors } from "./dialogs/space-dialog";
import { ruleError } from "./form/rules-editor";
import { syncSummary } from "./top-bar/sync-pill";
import { flowTooltipLabel } from "./workspace/flow-canvas";
import { safeTone } from "./workspace/source-card";

const peer = (state: PeerStatus["state"]): PeerStatus =>
  ({ id: state, name: state, address: "", os: "", state, progress: 0, last_synced: null, latency_ms: null, message: null }) as PeerStatus;
const sync = (states: PeerStatus["state"][], syncing = false): SyncStatus =>
  ({ listen_address: "", token: "", syncing, progress: 0, peers: states.map(peer) }) as SyncStatus;

describe("component helpers", () => {
  it("validates regex rules only", () => {
    expect(ruleError({ syntax: "regex", pattern: "(" } as never)).toBeTruthy();
    expect(ruleError({ syntax: "regex", pattern: "\\.arw$" } as never)).toBeNull();
    expect(ruleError({ syntax: "glob", pattern: "(" } as never)).toBeNull();
  });

  it("validates space variables", () => {
    const v = (name: string) => ({ name, default_value: "" }) as never;
    expect(variableErrors([v("backup_folder"), v("client")])).toEqual([]);
    expect(variableErrors([v("1abc")])).toHaveLength(1);
    expect(variableErrors([v("a"), v("a")])[0]).toContain("twice");
    expect(variableErrors([v("date")])[0]).toContain("built-in");
  });

  it("summarizes sync state", () => {
    expect(syncSummary(null).label).toBe("No peers");
    expect(syncSummary(sync(["error", "idle"])).label).toBe("Sync error on 1 peer");
    expect(syncSummary(sync(["syncing", "offline"], true)).label).toBe("Syncing 1 of 2 peers");
    expect(syncSummary(sync(["idle", "offline"])).label).toBe("1 of 2 peers online");
    expect(syncSummary(sync(["idle"])).label).toBe("All peers up to date");
  });

  it("describes flow link tooltip states with useful details", () => {
    const fs = (state: FlowStatus["state"], values: Partial<FlowStatus> = {}): FlowStatus => ({
      flow_id: "flow",
      state,
      transferred: 0,
      to_transfer: 0,
      ignored: 0,
      failed: 0,
      bytes_to_transfer: 0,
      error: null,
      ...values,
    });

    expect(flowTooltipLabel("done", fs("done", { transferred: 12 }))).toBe("Transferred · 12 files");
    expect(flowTooltipLabel("pending", fs("pending", { to_transfer: 3, bytes_to_transfer: 2500 }))).toBe("To transfer · 3 files · 2.5 KB");
    expect(flowTooltipLabel("error", fs("error", { failed: 1, error: "Hash mismatch" }))).toBe("Issue · 1 hash mismatch");
    expect(flowTooltipLabel("unavailable", fs("unavailable", { error: "Source offline" }))).toBe("Unavailable · Source offline");
  });

  it("picks the safe-copy tone", () => {
    expect(safeTone(2, 2)).toBe("badge-success");
    expect(safeTone(1, 2)).toBe("badge-warning");
    expect(safeTone(0, 2)).toBe("badge-error");
  });
});
