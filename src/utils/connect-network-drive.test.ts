import { describe, expect, it } from "vitest";
import type { DestinationStatus } from "../api/types";
import { canConnectNetworkDrive } from "./connect-network-drive";

describe("canConnectNetworkDrive", () => {
  const offline = { available: false } as DestinationStatus;
  it("offers connection only for confirmed offline NAS folder destinations on macOS", () => {
    expect(canConnectNetworkDrive("macos", "folder", "nas", offline)).toBe(true);
    expect(canConnectNetworkDrive("macos", undefined, "nas", offline)).toBe(true);
    expect(canConnectNetworkDrive("macos", "folder", "nas", { ...offline, available: true })).toBe(false);
    expect(canConnectNetworkDrive("macos", "folder", "nas", undefined)).toBe(false);
    expect(canConnectNetworkDrive("macos", "app", "nas", offline)).toBe(false);
    expect(canConnectNetworkDrive("macos", "folder", "ssd", offline)).toBe(false);
    expect(canConnectNetworkDrive("macos", "folder", undefined, offline)).toBe(false);
    for (const os of ["windows", "linux", "unknown"])
      expect(canConnectNetworkDrive(os, "folder", "nas", offline)).toBe(false);
  });
});
