import type { Destination, Source, Status } from "../../api/types";
import { destinationStatus, sourceStatus } from "../../state/derived";

export type DeviceFilter = { kind: "all" } | { kind: "mounted" } | { kind: "device"; deviceId: string };

export function filterDeviceCards(
  sources: Source[],
  destinations: Destination[],
  status: Status | null,
  filter: DeviceFilter,
) {
  if (filter.kind === "all") return { sources, destinations };
  return {
    sources: sources.filter((source) =>
      filter.kind === "device"
        ? source.device_id === filter.deviceId
        : !!source.device_id && !!sourceStatus(status, source.id)?.available,
    ),
    destinations: destinations.filter((destination) =>
      (destination.kind ?? "folder") === "app"
        ? true
        : filter.kind === "device"
          ? destination.device_id === filter.deviceId
          : !!destination.device_id && !!destinationStatus(status, destination.id)?.available,
    ),
  };
}
