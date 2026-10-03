import type { Device, Destination, Flow, Project, Snapshot, Source, Space } from "../types";

const space = (id: string, name: string, icon: string, position: number): Space => ({
  id,
  name,
  icon,
  position,
  hash_algo: "xxh64",
  verify_mode: "inline",
  variables: [
    { name: "project_name", default_value: "", required: true },
    { name: "backup_folder", default_value: "{year}/Travel", required: true },
    { name: "client", default_value: "Personal", required: false },
  ],
  backup_marker_template: "{date}_{project_name}",
});

const device = (id: string, name: string, description: string, role: Device["role"], kind: Device["kind"], capacity: number): Device => ({
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
});

const destination = (id: string, device_id: string, path_template: string, position: number): Destination => ({
  id,
  space_id: "travel",
  device_id,
  path_template,
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

const flow = (id: string, source_id: string, destination_id: string): Flow => ({ id, space_id: "travel", source_id, destination_id });

const projects: Project[] = [
  { id: "iceland", space_id: "travel", name: "Iceland 2026", values: { project_name: "Iceland_2026", backup_folder: "2026/Travel" }, final_copies_required: 2, archived: false },
  { id: "patagonia", space_id: "travel", name: "Patagonia 2025", values: { project_name: "Patagonia_2025" }, final_copies_required: 2, archived: true },
  { id: "wedding", space_id: "travel", name: "Wedding · Martin & Ana", values: { project_name: "Wedding_Martin_Ana", client: "Martin" }, final_copies_required: 2, archived: false },
];

export function demoSnapshot(): Snapshot {
  return structuredClone({
    computer: { id: "mbp", name: "MacBook Pro · Loïc", os: "macos" },
    computers: [
      { id: "mbp", name: "MacBook Pro · Loïc", os: "macos" },
      { id: "studio", name: "Studio PC", os: "windows" },
      { id: "nas-agent", name: "Home NAS agent", os: "linux" },
    ],
    spaces: [space("travel", "Travel", "plane", 0), space("home", "Home", "house", 1), space("backup", "Backup", "archive", 2)],
    projects,
    devices: [
      device("card1", "A7IV · Card 1", "SD card", "original", "sd_card", 128e9),
      device("card2", "A7IV · Card 2", "SD card", "original", "sd_card", 128e9),
      device("dji", "DJI Mini 4 Pro", "microSD", "original", "drone", 64e9),
      device("ssd", "Travel SSD", "Samsung T7", "temporary", "ssd", 2e12),
      device("nas", "Home NAS", "Synology DS923+ · via Netbird", "final", "nas", 16e12),
      device("hdd", "Archive HDD", "LaCie Rugged", "final", "hdd", 5e12),
    ],
    mappings: [
      { id: "card1@mbp", device_id: "card1", computer_id: "mbp", root_path: "/Volumes/A7IV_01" },
      { id: "card2@mbp", device_id: "card2", computer_id: "mbp", root_path: "/Volumes/A7IV_02" },
      { id: "dji@mbp", device_id: "dji", computer_id: "mbp", root_path: "/Volumes/DJI" },
      { id: "ssd@mbp", device_id: "ssd", computer_id: "mbp", root_path: "/Volumes/T7" },
      { id: "nas@mbp", device_id: "nas", computer_id: "mbp", root_path: "/Volumes/photo" },
      { id: "hdd@studio", device_id: "hdd", computer_id: "studio", root_path: "E:\\" },
    ],
    sources: [source("s1", "card1", "DCIM", 0), source("s2", "card2", "DCIM", 1), source("s3", "dji", "DCIM", 2)],
    destinations: [
      destination("d1", "ssd", "Projects/{project_name}/RAW", 0),
      destination("d2", "nas", "photo/{backup_folder}/{project_name}", 1),
      destination("d3", "hdd", "Archive/{project_name}", 2),
    ],
    flows: [flow("f1", "s1", "d1"), flow("f2", "s1", "d2"), flow("f3", "s2", "d1"), flow("f4", "s2", "d2"), flow("f5", "s3", "d1"), flow("f6", "s3", "d3")],
    settings: {
      theme: "system",
      auto_sync: true,
      auto_sync_minutes: 5,
      sync_port: 47821,
      active_space_id: "travel",
      active_project_by_space: { travel: "iceland" },
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
};

export const demoOffline = new Set(["hdd"]);
