import type { DestinationStatus, DeviceKind } from "../api/types";

export function destinationFreeBytes(
  kind: DeviceKind | undefined,
  status: Pick<DestinationStatus, "available" | "free_bytes"> | undefined,
): number | null {
  // Status uses local disk mount lookup, which can fall back to the host disk for NAS paths.
  if (kind === "nas" || !status?.available) return null;
  return status.free_bytes;
}
