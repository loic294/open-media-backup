import type { Destination, Device, Flow, Project, Source, Space, VariableDef } from "../api/types";
import { newId } from "../utils/id";

export function newSpace(name: string, position: number): Space {
  return {
    id: newId(),
    name,
    icon: "folder",
    position,
    hash_algo: "blake3",
    verify_mode: "reread",
    variables: [{ name: "project_name", default_value: "", required: true }],
    backup_marker_template: "{date}_{project_name}",
    allow_project_overlap: true,
    temporary_copies_per_final: 0,
  };
}

/** A project pre-filled with each variable's default value. */
export function newProject(space: Space, name: string): Project {
  const values = Object.fromEntries(space.variables.map((v: VariableDef) => [v.name, v.default_value]));
  return {
    id: newId(),
    space_id: space.id,
    name,
    values,
    final_copies_required: 2,
    archived: false,
    start_time: null,
    end_time: null,
    granularity: "minute",
    color: "#3b82f6",
  };
}

export function newDevice(name: string, kind: Device["kind"], role: Device["role"]): Device {
  return {
    id: newId(),
    name,
    description: "",
    role,
    kind,
    hw_serial: null,
    volume_uuid: null,
    capacity_bytes: null,
  };
}

export function newSource(spaceId: string, deviceId: string, position: number): Source {
  return {
    id: newId(),
    space_id: spaceId,
    device_id: deviceId,
    task_name: "",
    backup_name: "",
    path_template: "",
    offer_wipe: true,
    position,
    project_scope: { mode: "all" },
  };
}

/** Default rules skip camera housekeeping folders and sidecar thumbnails. */
export const DEFAULT_RULES: Destination["rules"] = [
  { action: "exclude", syntax: "glob", pattern: "PRIVATE/" },
  { action: "exclude", syntax: "glob", pattern: "*.THM" },
  { action: "exclude", syntax: "glob", pattern: ".*" },
];

export function newDestination(spaceId: string, deviceId: string, position: number): Destination {
  return {
    id: newId(),
    space_id: spaceId,
    kind: "folder",
    device_id: deviceId,
    task_name: "",
    path_template: "{project_name}",
    app_name: null,
    subfolder_per_source: true,
    preserve_file_structure: true,
    counts_as_safe_copy: true,
    use_backup_marker: false,
    rules: structuredClone(DEFAULT_RULES),
    position,
  };
}

export function newFlow(spaceId: string, sourceId: string, destinationId: string): Flow {
  return { id: newId(), space_id: spaceId, source_id: sourceId, destination_id: destinationId };
}
