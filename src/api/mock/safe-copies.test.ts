import { describe, expect, it } from "vitest";
import { demoSnapshot } from "./data";
import { mockSafeCopyDetails } from "./safe-copies";
import { mockStatus } from "./status";
import { createMockBackend } from "./mock-backend";

const context = { spaceId: "travel", projectId: "trip" };

function fixture() {
  const data = demoSnapshot();
  data.sources = [data.sources[0]];
  data.flows = data.flows.filter((flow) => flow.source_id === "s1" && flow.destination_id !== "d4");
  data.destinations.forEach((dest) => {
    dest.rules = [];
  });
  data.spaces[0].final_copies_required = 2;
  return data;
}

describe("demo file safety", () => {
  it("reports completed copies from non-excluded files, not excluded pending files", () => {
    const data = fixture();
    const counts = { f1: [3, 0, 0, 0], f2: [1, 2, 0, 0] } satisfies import("./status").Counts;
    const pending = mockSafeCopyDetails(data, { ...context, projectId: null }, "s1", counts, new Set());
    expect(pending.safe_copies).toBe(1);
    expect(pending.files.filter((file) => file.state === "unsafe")).toHaveLength(2);
    data.sources[0].safe_copy_rules = pending.files
      .filter((file) => file.state === "unsafe")
      .map((file) => ({ action: "exclude", syntax: "glob", pattern: file.path.replace(/^DCIM\//, "") }));
    const completed = mockSafeCopyDetails(data, { ...context, projectId: null }, "s1", counts, new Set());
    expect(completed.files.filter((file) => file.state === "unsafe")).toHaveLength(0);
    expect(completed.safe_copies).toBe(2);
    expect(completed.required_copies).toBe(2);
    expect(completed.wipe_eligible).toBe(true);
    data.sources[0].safe_copy_rules = [{ action: "exclude", syntax: "glob", pattern: "*" }];
    expect(mockSafeCopyDetails(data, context, "s1", counts, new Set()).safe_copies).toBe(2);
  });

  it("uses one space threshold with or without projects and rejects invalid policy saves", async () => {
    const data = fixture();
    data.projects = [];
    data.spaces[0].final_copies_required = 1;
    const details = mockSafeCopyDetails(
      data,
      { ...context, projectId: null },
      "s1",
      { f1: [1, 0, 0, 0], f2: [0, 1, 0, 0] },
      new Set(),
    );
    expect(details.required_copies).toBe(1);
    expect(details.wipe_eligible).toBe(true);
    const backend = createMockBackend({ seedRunningTransfer: false });
    for (const invalid of [0, -1, 1.5, 4294967296]) {
      await expect(
        backend.saveEntity("space", { ...data.spaces[0], final_copies_required: invalid }),
      ).rejects.toThrow("Required copies");
    }
  });

  it("separates verified, pending, offline and deliberate skip evidence", () => {
    const data = fixture();
    data.spaces[0].skip_counts_as_safe_copy = true;
    const details = mockSafeCopyDetails(
      data,
      context,
      "s1",
      { f1: [3, 0, 0, 0], f2: [1, 2, 0, 0] },
      new Set(["nas"]),
      { f2: 1 },
    );
    expect(details.files.map((file) => file.state)).toEqual(["unsafe", "safe", "safe"]);
    const skipped = details.files.find((file) => file.acknowledged_destinations.length)!;
    expect(skipped.acknowledged_destinations).toEqual(["Home NAS"]);
    expect(skipped.verified_destinations).toEqual(["Travel SSD"]);
    expect(details.files.find((file) => file.state === "unsafe")?.reasons.join(" ")).toContain("offline");
    data.spaces[0].skip_counts_as_safe_copy = false;
    expect(
      mockSafeCopyDetails(data, context, "s1", { f1: [3, 0, 0, 0], f2: [1, 2, 0, 0] }, new Set(), {
        f2: 1,
      }).files.filter((file) => file.state === "safe"),
    ).toHaveLength(1);
  });

  it("explicit exclusions remove pending requirements and change wipe readiness", () => {
    const data = fixture();
    const counts = { f1: [1, 0, 0, 0], f2: [0, 1, 0, 0] } satisfies import("./status").Counts;
    expect(mockStatus(data, "trip", counts, new Set()).sources[0].wipe_eligible).toBe(false);
    data.sources[0].safe_copy_rules = [{ action: "exclude", syntax: "glob", pattern: "*" }];
    const details = mockSafeCopyDetails(data, context, "s1", counts, new Set());
    expect(details.files[0].state).toBe("excluded");
    expect(details.wipe_eligible).toBe(true);
    expect(mockStatus(data, "trip", counts, new Set()).sources[0].wipe_eligible).toBe(true);
    data.sources.push({ ...data.sources[0], id: "overlap", safe_copy_rules: [] });
    data.flows.push({ ...data.flows[0], id: "overlap-flow", source_id: "overlap" });
    expect(
      mockSafeCopyDetails(data, context, "s1", { ...counts, "overlap-flow": [0, 1, 0, 0] }, new Set())
        .files[0].state,
    ).toBe("unsafe");
  });

  it("reports missing destination rules without inventing coverage", () => {
    const data = fixture();
    data.destinations.forEach((dest) => {
      dest.rules = [{ action: "exclude", syntax: "glob", pattern: "*" }];
    });
    const details = mockSafeCopyDetails(
      data,
      context,
      "s1",
      { f1: [1, 0, 0, 0], f2: [1, 0, 0, 0] },
      new Set(),
    );
    expect(details.files[0].state).toBe("unsafe");
    expect(details.files[0].reasons.join(" ")).toContain("rules exclude");
    expect(details.safe_copies).toBe(0);
  });

  it("persists synced source rules, emits updates and rejects remote/invalid editing", async () => {
    const backend = createMockBackend({ seedRunningTransfer: false });
    const changed = { count: 0 };
    await backend.on("snapshot-changed", () => changed.count++);
    const rules = [{ action: "exclude", syntax: "glob", pattern: "*.THM" }] as const;
    await backend.saveSourceSafeCopyRules(context, "s1", [...rules]);
    expect((await backend.getSnapshot()).sources[0].safe_copy_rules).toEqual(rules);
    expect(changed.count).toBe(1);
    await expect(
      backend.saveSourceSafeCopyRules(context, "s1", [{ action: "exclude", syntax: "regex", pattern: "[" }]),
    ).rejects.toThrow();
    const snapshot = await backend.getSnapshot();
    await backend.saveEntity("source", {
      ...snapshot.sources[0],
      id: "remote",
      device_id: "hdd",
      safe_copy_rules: [],
    });
    await expect(backend.saveSourceSafeCopyRules(context, "remote", [...rules])).rejects.toThrow(
      "mapped on this computer",
    );
    await expect(
      backend.getSourceSafeCopyDetails({ spaceId: "home", projectId: null }, "s1"),
    ).rejects.toThrow("workspace space");
  });
});
