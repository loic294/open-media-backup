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
  return destinations.filter((destination) =>
    (destination.kind ?? "folder") === "app"
      ? true
      : filter.kind === "device"
        ? destination.device_id === filter.deviceId
        : !!destination.device_id && !!destinationStatus(status, destination.id)?.available,
  );
}
