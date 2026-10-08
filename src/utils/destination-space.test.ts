import { describe, expect, it } from "vitest";
import type { DeviceKind } from "../api/types";
import { destinationFreeBytes } from "./destination-space";

describe("destinationFreeBytes", () => {
  it("suppresses NAS space even when the destination is available", () => {
    expect(destinationFreeBytes("nas", { available: true, free_bytes: 1.2e12 })).toBeNull();
  });

  it.each<DeviceKind>(["sd_card", "ssd", "hdd", "computer", "camera", "drone", "other"])(
    "preserves measured free space for %s destinations, including full disks",
    (kind) => {
      expect(destinationFreeBytes(kind, { available: true, free_bytes: 1.2e12 })).toBe(1.2e12);
      expect(destinationFreeBytes(kind, { available: true, free_bytes: 0 })).toBe(0);
    },
  );

  it("preserves unknown, offline and missing status behavior", () => {
    expect(destinationFreeBytes(undefined, { available: true, free_bytes: 12 })).toBe(12);
    expect(destinationFreeBytes("ssd", { available: false, free_bytes: 12 })).toBeNull();
    expect(destinationFreeBytes("ssd", { available: true, free_bytes: null })).toBeNull();
    expect(destinationFreeBytes("ssd", undefined)).toBeNull();
  });
});
