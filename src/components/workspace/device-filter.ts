import type { Destination, Source, Status } from "../../api/types";
import { destinationStatus, sourceStatus } from "../../state/derived";

export type DeviceFilter = { kind: "all" } | { kind: "mounted" } | { kind: "device"; deviceId: string };

export function filterSources(sources: Source[], status: Status | null, filter: DeviceFilter): Source[] {
  if (filter.kind === "all") return sources;
  return sources.filter((source) =>
    filter.kind === "device"
      ? source.device_id === filter.deviceId
      : !!source.device_id && !!sourceStatus(status, source.id)?.available,
  );
}

export function filterDestinations(
  destinations: Destination[],
  status: Status | null,
  filter: DeviceFilter,
): Destination[] {
  if (filter.kind === "all") return destinations;
  return destinations.filter((destination) => {
    // Manual apps are always visible, including under Mounted devices; this does not change their status.
    if (destination.kind === "app") return true;
    if (filter.kind === "device") return destination.device_id === filter.deviceId;
    return !!destination.device_id && !!destinationStatus(status, destination.id)?.available;
  });
}
