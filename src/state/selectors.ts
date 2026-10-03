import type { Device, DeviceMapping, Destination, Flow, Project, Snapshot, Source, Space } from "../api/types";

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

export function activeProject(s: Snapshot, spaceId: string | undefined): Project | null {
  if (!spaceId) return null;
  const projects = spaceProjects(s, spaceId);
  const chosen = s.settings.active_project_by_space[spaceId];
  return projects.find((p) => p.id === chosen) ?? projects.find((p) => !p.archived) ?? null;
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

export function deviceById(s: Snapshot, id: string): Device | undefined {
  return s.devices.find((d) => d.id === id);
}

/** Where a device lives on a given computer (this one by default). */
export function mappingFor(s: Snapshot, deviceId: string, computerId = s.computer.id): DeviceMapping | undefined {
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

/** "Camera A · Card 1 → Home NAS" */
export function flowLabel(s: Snapshot, flow: Flow): string {
  const src = s.sources.find((x) => x.id === flow.source_id);
  const dst = s.destinations.find((x) => x.id === flow.destination_id);
  const name = (deviceId?: string) => (deviceId && deviceById(s, deviceId)?.name) || "?";
  return `${name(src?.device_id)} → ${name(dst?.device_id)}`;
}
