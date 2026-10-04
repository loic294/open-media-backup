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
    expect(mockStatus(data, "project", { "app-flow": [0, 1, 0, 0] }, new Set()).sources[0].safe_copies).toBe(0);
    const source = mockStatus(data, "project", completed, new Set()).sources[0];
    expect(source.safe_copies).toBe(1);
    expect(source.wipe_eligible).toBe(true);
  });
});
