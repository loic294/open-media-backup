import type { Device, Destination, Flow, Project, Snapshot, Source, Space } from "../types";

const space = (id: string, name: string, icon: string, position: number): Space => ({
  id,
  name,
  icon,
  position,
  hash_algo: "blake3",
  verify_mode: "reread",
  variables: [
    { name: "project_name", default_value: "", required: true },
    { name: "backup_folder", default_value: "{year}/Travel", required: true },
    { name: "client", default_value: "Personal", required: false },
  ],
  backup_marker_template: "{date}_{project_name}",
  allow_project_overlap: true,
  temporary_copies_per_final: id === "travel" ? 1 : 0,
});

const device = (
  id: string,
  name: string,
  description: string,
  role: Device["role"],
  kind: Device["kind"],
  capacity: number,
): Device => ({
  id,
  name,
  description,
  role,
  kind,
  hw_serial: null,
  volume_uuid: null,
  capacity_bytes: capacity,
});

const source = (id: string, device_id: string, path_template: string, position: number): Source => ({
  id,
  space_id: "travel",
  device_id,
  path_template,
  offer_wipe: true,
  position,
  project_scope: { mode: "all" },
});

const destination = (
  id: string,
  device_id: string,
  path_template: string,
  position: number,
): Destination => ({
  id,
  space_id: "travel",
  kind: "folder",
  device_id,
  path_template,
  app_name: null,
  subfolder_per_source: true,
  counts_as_safe_copy: true,
  use_backup_marker: false,
  rules: [
    { action: "include", syntax: "glob", pattern: "*" },
    { action: "exclude", syntax: "glob", pattern: "PRIVATE/" },
    { action: "exclude", syntax: "glob", pattern: "*.THM" },
  ],
  position,
});

const appDestination = (id: string, appName: string, position: number): Destination => ({
  id,
  space_id: "travel",
  kind: "app",
  device_id: "",
  path_template: "",
  app_name: appName,
  subfolder_per_source: false,
  counts_as_safe_copy: false,
  use_backup_marker: false,
  rules: [
    { action: "include", syntax: "glob", pattern: "*.JPG" },
    { action: "include", syntax: "glob", pattern: "*.jpg" },
  ],
  position,
});

const flow = (id: string, source_id: string, destination_id: string): Flow => ({
  id,
  space_id: "travel",
  source_id,
  destination_id,
});

const legacyProjects: Project[] = [
  {
    id: "trip",
    space_id: "travel",
    name: "Trip 2026",
    values: { project_name: "Trip_2026", backup_folder: "2026/Travel" },
    final_copies_required: 2,
    archived: false,
  },
  {
    id: "trip-2025",
    space_id: "travel",
    name: "Trip 2025",
    values: { project_name: "Trip_2025" },
    final_copies_required: 2,
    archived: true,
  },
  {
    id: "event",
    space_id: "travel",
    name: "Client event",
    values: { project_name: "Client_event", client: "Client" },
    final_copies_required: 2,
    archived: false,
  },
];
const projects: Project[] = legacyProjects.map((project) => {
  const ranges = {
    trip: ["2026-01-12T08:00:00Z", "2026-01-16T23:59:00Z", "#A78BFA"],
    "trip-2025": ["2025-12-02T00:00:00Z", "2025-12-12T23:59:00Z", "#FB923C"],
    event: ["2026-01-14T09:00:00Z", "2026-01-14T23:30:00Z", "#38BDF8"],
  } as const;
  const [start, end, color] = ranges[project.id as keyof typeof ranges];
  return {
    ...project,
    start_time: Date.parse(start),
    end_time: Date.parse(end),
    granularity: "day",
    color,
  };
});

export function demoSnapshot(): Snapshot {
  return structuredClone({
    computer: { id: "laptop", name: "Laptop", os: "macos" },
    computers: [
      { id: "laptop", name: "Laptop", os: "macos" },
      { id: "studio", name: "Desktop PC", os: "windows" },
      { id: "nas-agent", name: "Home NAS agent", os: "linux" },
    ],
    spaces: [
      space("travel", "Travel", "plane", 0),
      space("home", "Home", "house", 1),
      space("backup", "Backup", "archive", 2),
    ],
    projects,
    devices: [
      device("card1", "Camera A · Card 1", "SD card", "original", "sd_card", 128e9),
      device("card2", "Camera A · Card 2", "SD card", "original", "sd_card", 128e9),
      device("dji", "Drone card", "microSD", "original", "drone", 64e9),
      device("ssd", "Travel SSD", "Portable SSD", "temporary", "ssd", 2e12),
      device("nas", "Home NAS", "Network storage", "final", "nas", 16e12),
      device("hdd", "Archive HDD", "USB drive", "final", "hdd", 5e12),
    ],
    mappings: [
      { id: "card1@laptop", device_id: "card1", computer_id: "laptop", root_path: "/Volumes/CAM_A_01" },
      { id: "card2@laptop", device_id: "card2", computer_id: "laptop", root_path: "/Volumes/CAM_A_02" },
      { id: "dji@laptop", device_id: "dji", computer_id: "laptop", root_path: "/Volumes/DRONE" },
      { id: "ssd@laptop", device_id: "ssd", computer_id: "laptop", root_path: "/Volumes/T7" },
      { id: "nas@laptop", device_id: "nas", computer_id: "laptop", root_path: "/Volumes/photo" },
      { id: "hdd@studio", device_id: "hdd", computer_id: "studio", root_path: "E:\\" },
    ],
    sources: [
      source("s1", "card1", "DCIM", 0),
      source("s2", "card2", "DCIM", 1),
      source("s3", "dji", "DCIM", 2),
    ],
    destinations: [
      destination("d1", "ssd", "Projects/{project_name}/RAW", 0),
      destination("d2", "nas", "photo/{backup_folder}/{project_name}", 1),
      destination("d3", "hdd", "Archive/{project_name}", 2),
      appDestination("d4", "Lightroom", 3),
    ],
    flows: [
      flow("f1", "s1", "d1"),
      flow("f2", "s1", "d2"),
      flow("f3", "s2", "d1"),
      flow("f4", "s2", "d2"),
      flow("f5", "s3", "d1"),
      flow("f6", "s3", "d3"),
      flow("f7", "s1", "d4"),
    ],
    settings: {
      theme: "system",
      auto_sync: true,
      auto_sync_minutes: 5,
      sync_port: 47821,
      active_space_id: "travel",
      active_project_by_space: { travel: "trip" },
      preview_apps: { photos: null, videos: null },
      app_destinations: { d4: "/Applications/Adobe Lightroom.app" },
      transfer_speeds: { ssd: 185_000_000, nas: 92_000_000, _global: 140_000_000 },
    },
  } satisfies Snapshot);
}

/** Simulated per-flow file counts: [transferred, to_transfer, ignored, failed]. */
export const demoCounts: Record<string, [number, number, number, number]> = {
  f1: [1248, 0, 0, 0],
  f2: [1248, 0, 52, 0],
  f3: [1061, 0, 36, 0],
  f4: [0, 1061, 0, 0],
  f5: [0, 0, 0, 3],
  f6: [0, 302, 16, 0],
  f7: [0, 1061, 319, 0],
};

export const demoOffline = new Set(["hdd"]);
