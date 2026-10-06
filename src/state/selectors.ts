import type {
  Device,
  DeviceMapping,
  Destination,
  Flow,
  Project,
  Snapshot,
  Source,
  Space,
  Volume,
} from "../api/types";
import { destinationTaskName, sourceTaskName } from "../utils/names";
import { configuredDestinationApp } from "../utils/preview-apps";
export { matchingProjects } from "./projects";

export function sortedSpaces(s: Snapshot): Space[] {
  return [...s.spaces].sort((a, b) => a.position - b.position || a.name.localeCompare(b.name));
}

export function activeSpace(s: Snapshot): Space | null {
  const spaces = sortedSpaces(s);
  return spaces.find((x) => x.id === s.settings.active_space_id) ?? spaces[0] ?? null;
}

/** Projects of a space, active ones first. */
export function spaceProjects(s: Snapshot, spaceId: string): Project[] {
  return s.projects
    .filter((p) => p.space_id === spaceId)
    .sort((a, b) => Number(a.archived) - Number(b.archived) || a.name.localeCompare(b.name));
}

const byPosition = <T extends { position: number }>(a: T, b: T) => a.position - b.position;

export function spaceSources(s: Snapshot, spaceId: string): Source[] {
  return s.sources.filter((x) => x.space_id === spaceId).sort(byPosition);
}

export function spaceDestinations(s: Snapshot, spaceId: string): Destination[] {
  return s.destinations.filter((x) => x.space_id === spaceId).sort(byPosition);
}

export function spaceFlows(s: Snapshot, spaceId: string): Flow[] {
  return s.flows.filter((x) => x.space_id === spaceId);
}

/** Stable ordering for items whose associated device is currently mounted here. */
export function mountedFirst<T>(items: T[], volumes: Volume[], deviceId: (item: T) => string | null): T[] {
  const mounted = new Set(volumes.flatMap((volume) => (volume.device_id ? [volume.device_id] : [])));
  return [...items].sort(
    (a, b) => Number(mounted.has(deviceId(b) ?? "")) - Number(mounted.has(deviceId(a) ?? "")),
  );
}

/** Sort device-backed items while keeping items without a device in their current slots. */
export function mountedFirstInSlots<T>(
  items: T[],
  volumes: Volume[],
  deviceId: (item: T) => string | null,
): T[] {
  const positions: number[] = [];
  const deviceItems: T[] = [];
  items.forEach((item, index) => {
    if (deviceId(item)) {
      positions.push(index);
      deviceItems.push(item);
    }
  });
  const sorted = mountedFirst(deviceItems, volumes, deviceId);
  const result = [...items];
  positions.forEach((position, index) => {
    result[position] = sorted[index];
  });
  return result;
}

export function deviceById(s: Snapshot, id: string): Device | undefined {
  return s.devices.find((d) => d.id === id);
}

/** Where a device lives on a given computer (this one by default). */
export function mappingFor(
  s: Snapshot,
  deviceId: string,
  computerId = s.computer.id,
): DeviceMapping | undefined {
  return s.mappings.find((m) => m.device_id === deviceId && m.computer_id === computerId);
}

/** Other computers that know this device, e.g. "on Desktop PC". */
export function deviceHosts(s: Snapshot, deviceId: string): string[] {
  return s.mappings
    .filter((m) => m.device_id === deviceId && m.computer_id !== s.computer.id)
    .map((m) => s.computers.find((c) => c.id === m.computer_id)?.name ?? m.computer_id);
}

export function nextPosition(items: { position: number }[]): number {
  return items.reduce((max, i) => Math.max(max, i.position + 1), 0);
}

/** Display-only task names, falling back to device/application names. */
export function flowLabel(s: Snapshot, flow: Flow): string {
  const src = s.sources.find((x) => x.id === flow.source_id);
  const dst = s.destinations.find((x) => x.id === flow.destination_id);
  const sourceName = src ? sourceTaskName(src, deviceById(s, src.device_id)) : "?";
  const destinationName = dst
    ? destinationTaskName(dst, deviceById(s, dst.device_id), configuredDestinationApp(s.settings, dst.id))
    : "?";
  return `${sourceName} → ${destinationName}`;
}
