import type { Destination, DeviceKind, DestinationStatus } from "../api/types";

export function canConnectNetworkDrive(
  os: string,
  destinationKind: Destination["kind"],
  deviceKind: DeviceKind | undefined,
  status: DestinationStatus | undefined,
): boolean {
  return os === "macos" && destinationKind !== "app" && deviceKind === "nas" && status?.available === false;
}
