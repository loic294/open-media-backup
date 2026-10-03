import { describe, expect, it } from "vitest";
import type { PeerStatus, SyncStatus } from "../api/types";
import { variableErrors } from "./dialogs/space-dialog";
import { ruleError } from "./form/rules-editor";
import { syncSummary } from "./top-bar/sync-pill";
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

  it("picks the safe-copy tone", () => {
    expect(safeTone(2, 2)).toBe("badge-success");
    expect(safeTone(1, 2)).toBe("badge-warning");
    expect(safeTone(0, 2)).toBe("badge-error");
  });
});
