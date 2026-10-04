import { describe, expect, it } from "vitest";
import type { Snapshot } from "../types";
import type { Counts } from "./status";
import { mockStatus } from "./status";

const snapshot = (
  temporaryCopiesPerFinal = 0,
  sourceRole: "original" | "temporary" = "original",
): Snapshot => ({
  computer: { id: "computer", name: "Computer", os: "macos" },
  computers: [],
  spaces: [
    {
      id: "space",
      name: "Space",
      icon: "folder",
      position: 0,
      hash_algo: "blake3",
      verify_mode: "reread",
      variables: [],
      backup_marker_template: "",
      allow_project_overlap: true,
      temporary_copies_per_final: temporaryCopiesPerFinal,
    },
  ],
  projects: [
    {
      id: "project",
      space_id: "space",
      name: "Project",
      values: {},
      final_copies_required: 2,
      archived: false,
    },
  ],
  devices: [
    {
      id: "source",
      name: "Source",
      description: "",
      role: sourceRole,
      kind: "sd_card",
      hw_serial: null,
      volume_uuid: null,
      capacity_bytes: null,
    },
    {
      id: "final",
      name: "Final",
      description: "",
      role: "final",
      kind: "nas",
      hw_serial: null,
      volume_uuid: null,
      capacity_bytes: null,
    },
    {
      id: "tmp1",
      name: "Tmp 1",
      description: "",
      role: "temporary",
      kind: "ssd",
      hw_serial: null,
      volume_uuid: null,
      capacity_bytes: null,
    },
    {
      id: "tmp2",
      name: "Tmp 2",
      description: "",
      role: "temporary",
      kind: "ssd",
      hw_serial: null,
      volume_uuid: null,
      capacity_bytes: null,
    },
  ],
  mappings: [],
  sources: [
    {
      id: "source",
      space_id: "space",
      device_id: "source",
      path_template: "",
      offer_wipe: true,
      position: 0,
    },
  ],
  destinations: [
    {
      id: "final-dest",
      space_id: "space",
      device_id: "final",
      path_template: "",
      subfolder_per_source: true,
      preserve_file_structure: true,
      counts_as_safe_copy: true,
      use_backup_marker: false,
      rules: [],
      position: 0,
    },
    {
      id: "tmp-dest-1",
      space_id: "space",
      device_id: "tmp1",
      path_template: "",
      subfolder_per_source: true,
      preserve_file_structure: true,
      counts_as_safe_copy: true,
      use_backup_marker: false,
      rules: [],
      position: 1,
    },
    {
      id: "tmp-dest-2",
      space_id: "space",
      device_id: "tmp2",
      path_template: "",
      subfolder_per_source: true,
      preserve_file_structure: true,
      counts_as_safe_copy: true,
      use_backup_marker: false,
      rules: [],
      position: 2,
    },
  ],
  flows: [
    { id: "final-flow", space_id: "space", source_id: "source", destination_id: "final-dest" },
    { id: "tmp-flow-1", space_id: "space", source_id: "source", destination_id: "tmp-dest-1" },
    { id: "tmp-flow-2", space_id: "space", source_id: "source", destination_id: "tmp-dest-2" },
  ],
  settings: {
    theme: "system",
    auto_sync: false,
    auto_sync_minutes: 5,
    sync_port: 0,
    active_space_id: "space",
    active_project_by_space: { space: "project" },
    preview_apps: { photos: null, videos: null },
  },
});

describe("mockStatus temporary safe copies", () => {
  it("groups temporary destination copies according to the space setting", () => {
    const counts: Counts = {
      "final-flow": [1, 0, 0, 0],
      "tmp-flow-1": [1, 0, 0, 0],
      "tmp-flow-2": [1, 0, 0, 0],
    };

    expect(mockStatus(snapshot(0), "project", counts, new Set()).sources[0].safe_copies).toBe(1);
    expect(mockStatus(snapshot(2), "project", counts, new Set()).sources[0].safe_copies).toBe(2);
  });

  it("keeps temporary sources blocked until a final destination copy is complete", () => {
    const counts: Counts = {
      "final-flow": [0, 1, 0, 0],
      "tmp-flow-1": [1, 0, 0, 0],
      "tmp-flow-2": [1, 0, 0, 0],
    };

    const source = mockStatus(snapshot(2, "temporary"), "project", counts, new Set()).sources[0];

    expect(source.safe_copies).toBe(1);
    expect(source.wipe_eligible).toBe(false);
    expect(source.blocking_reason).toBe("Needs a final destination");
  });
});

describe("mockStatus app safe copies", () => {
  it("counts only completed imports from opted-in app destinations", () => {
    const data = snapshot();
    data.projects[0].final_copies_required = 1;
    data.destinations.push({
      id: "app-dest",
      space_id: "space",
      kind: "app",
      device_id: "",
      path_template: "",
      app_name: "Photo app",
      subfolder_per_source: false,
      preserve_file_structure: true,
      counts_as_safe_copy: false,
      use_backup_marker: false,
      rules: [],
      position: 3,
    });
    data.flows.push({
      id: "app-flow",
      space_id: "space",
      source_id: "source",
      destination_id: "app-dest",
    });

    const completed = { "app-flow": [1, 0, 0, 0] as [number, number, number, number] };
    expect(mockStatus(data, "project", completed, new Set()).sources[0].safe_copies).toBe(0);

    data.destinations.at(-1)!.counts_as_safe_copy = true;
    expect(mockStatus(data, "project", { "app-flow": [0, 1, 0, 0] }, new Set()).sources[0].safe_copies).toBe(
      0,
    );
    const source = mockStatus(data, "project", completed, new Set()).sources[0];
    expect(source.safe_copies).toBe(1);
    expect(source.wipe_eligible).toBe(true);
  });
});

function groupedSnapshot(temporaryCopiesPerFinal = 0): Snapshot {
  const data = snapshot(temporaryCopiesPerFinal);
  data.projects[0].final_copies_required = 1;
  data.sources.push({ ...data.sources[0], id: "sibling", path_template: "VIDEO", position: 1 });
  data.flows.push(...data.flows.map((flow) => ({ ...flow, id: `sibling-${flow.id}`, source_id: "sibling" })));
  return data;
}

describe("mockStatus device-level safe copies", () => {
  it("shares counts and wipe safety until both sources are copied to the same device", () => {
    const data = groupedSnapshot();
    const counts: Counts = {
      "final-flow": [1, 0, 0, 0],
      "sibling-final-flow": [0, 1, 0, 0],
    };
    const pending = mockStatus(data, "project", counts, new Set()).sources;
    expect(pending.map((source) => source.safe_copies)).toEqual([0, 0]);
    expect(pending.every((source) => !source.wipe_eligible)).toBe(true);
    counts["sibling-final-flow"] = [2, 0, 0, 0];
    const complete = mockStatus(data, "project", counts, new Set()).sources;
    expect(complete.map((source) => source.safe_copies)).toEqual([1, 1]);
    expect(complete.every((source) => source.wipe_eligible)).toBe(true);
  });

  it("does not combine disjoint target coverage or count a target missing sibling coverage", () => {
    const data = groupedSnapshot();
    data.devices.push({ ...data.devices[1], id: "other-final" });
    data.destinations.push({ ...data.destinations[0], id: "other-dest", device_id: "other-final" });
    data.flows.push({
      id: "other-flow",
      space_id: "space",
      source_id: "sibling",
      destination_id: "other-dest",
    });
    const counts: Counts = {
      "final-flow": [1, 0, 0, 0],
      "other-flow": [1, 0, 0, 0],
    };
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([0, 0]);
    data.flows = data.flows.filter((flow) => flow.id !== "sibling-final-flow");
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([0, 0]);
  });

  it("counts duplicate folder destinations on the same device only once", () => {
    const data = groupedSnapshot();
    data.destinations.push({ ...data.destinations[0], id: "another-folder" });
    data.flows.push(
      ...data.sources.map((source) => ({
        id: `duplicate-${source.id}`,
        space_id: "space",
        source_id: source.id,
        destination_id: "another-folder",
      })),
    );
    const counts: Counts = {
      "final-flow": [1, 0, 0, 0],
      "sibling-final-flow": [1, 0, 0, 0],
      "duplicate-source": [1, 0, 0, 0],
      "duplicate-sibling": [1, 0, 0, 0],
    };
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([1, 1]);
  });

  it("converts only complete distinct temporary devices after grouping", () => {
    const data = groupedSnapshot(2);
    const counts: Counts = {
      "tmp-flow-1": [1, 0, 0, 0],
      "sibling-tmp-flow-1": [1, 0, 0, 0],
      "tmp-flow-2": [1, 0, 0, 0],
      "sibling-tmp-flow-2": [0, 1, 0, 0],
    };
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([0, 0]);
    counts["sibling-tmp-flow-2"] = [1, 0, 0, 0];
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([1, 1]);
    data.devices[0].role = "temporary";
    expect(mockStatus(data, "project", counts, new Set()).sources.every((s) => !s.wipe_eligible)).toBe(true);
  });

  it("requires opted-in app confirmation across the whole group", () => {
    const data = groupedSnapshot();
    data.destinations.push({
      ...data.destinations[0],
      id: "app",
      kind: "app",
      device_id: "",
      app_name: "Photos",
    });
    data.flows.push(
      ...data.sources.map((source) => ({
        id: `app-${source.id}`,
        source_id: source.id,
        destination_id: "app",
        space_id: "space",
      })),
    );
    const counts: Counts = { "app-source": [1, 0, 0, 0] };
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([0, 0]);
    counts["app-sibling"] = [1, 0, 0, 0];
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([1, 1]);
    data.destinations.at(-1)!.counts_as_safe_copy = false;
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([0, 0]);
  });

  it("ignores empty siblings but blocks every source when a sibling path cannot resolve", () => {
    const data = groupedSnapshot();
    const counts: Counts = { "final-flow": [1, 0, 0, 0] };
    for (const flow of data.flows.filter((flow) => flow.source_id === "sibling"))
      counts[flow.id] = [0, 0, 0, 0];
    const sources = mockStatus(data, "project", counts, new Set()).sources;
    expect(sources.map((s) => s.safe_copies)).toEqual([1, 1]);
    expect(sources[0].wipe_eligible).toBe(true);
    expect(sources[1].wipe_eligible).toBe(false);
    data.sources[1].path_template = "{unknown}";
    const blocked = mockStatus(data, "project", counts, new Set()).sources;
    expect(blocked.every((s) => !s.wipe_eligible && s.blocking_reason?.includes("sibling"))).toBe(true);
  });

  it.each(["VIDEO/{unknown", "VIDEO/{project_name}"])(
    "blocks grouped wiping for a malformed or empty-value path: %s",
    (path) => {
      const data = groupedSnapshot();
      data.projects[0].name = "";
      data.sources[1].path_template = path;
      const counts: Counts = {
        "final-flow": [1, 0, 0, 0],
        "sibling-final-flow": [1, 0, 0, 0],
      };
      expect(
        mockStatus(data, "project", counts, new Set()).sources.every(
          (source) => !source.wipe_eligible && source.blocking_reason?.includes("sibling"),
        ),
      ).toBe(true);
    },
  );

  it("keeps different source devices and spaces independent and excludes self-copies", () => {
    const data = groupedSnapshot();
    data.sources[1].device_id = "tmp1";
    const counts: Counts = { "final-flow": [1, 0, 0, 0] };
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([1, 0]);
    data.sources.push({
      ...data.sources[0],
      id: "elsewhere",
      space_id: "other-space",
      path_template: "{unknown}",
    });
    expect(mockStatus(data, "project", counts, new Set()).sources[0].wipe_eligible).toBe(true);
    data.destinations[0].device_id = "source";
    data.devices[0].role = "final";
    expect(mockStatus(data, "project", counts, new Set()).sources[0].safe_copies).toBe(0);
  });

  it("does not count failed or all-ignored coverage as a complete copy", () => {
    const data = groupedSnapshot();
    const counts: Counts = {
      "final-flow": [1, 0, 0, 0],
      "sibling-final-flow": [0, 0, 0, 1],
    };
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([0, 0]);
    counts["final-flow"] = [0, 0, 1, 0];
    counts["sibling-final-flow"] = [0, 0, 1, 0];
    expect(mockStatus(data, "project", counts, new Set()).sources.map((s) => s.safe_copies)).toEqual([0, 0]);
  });
});
