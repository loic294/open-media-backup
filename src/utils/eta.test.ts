import { describe, expect, it } from "vitest";
import { estimateTransferSeconds, formatSpeed, learnedTransferSpeed, liveTransferSpeed } from "./eta";

describe("eta helpers", () => {
  it("reads live transfer speed from the new payload field", () => {
    expect(liveTransferSpeed({ bytes_per_sec: 42, speed_bps: 10 })).toBe(42);
    expect(liveTransferSpeed({ bytes_per_sec: null, speed_bps: 10 })).toBe(10);
    expect(liveTransferSpeed({ bytes_per_sec: 0, speed_bps: 0 })).toBeNull();
  });

  it("prefers destination speed over global and default", () => {
    expect(learnedTransferSpeed({ nas: 50, _global: 20 }, "nas", 10)).toBe(50);
    expect(learnedTransferSpeed({ _global: 20 }, "nas", 10)).toBe(20);
    expect(learnedTransferSpeed({}, "nas", 10)).toBe(10);
  });

  it("estimates pending transfer seconds from learned speeds", () => {
    expect(estimateTransferSeconds(1_000, { nas: 100 }, "nas")).toBe(10);
    expect(estimateTransferSeconds(1_000, { _global: 200 }, "nas")).toBe(5);
    expect(estimateTransferSeconds(0, { nas: 100 }, "nas")).toBeNull();
  });

  it("ignores non-positive learned speeds", () => {
    expect(learnedTransferSpeed({ nas: 0, _global: -1 }, "nas", 10)).toBe(10);
  });

  it("formats live transfer speeds", () => {
    expect(formatSpeed(842)).toBe("842 B/s");
    expect(formatSpeed(84_200)).toBe("84.2 KB/s");
    expect(formatSpeed(84_200_000)).toBe("84.2 MB/s");
    expect(formatSpeed(1_200_000_000)).toBe("1.2 GB/s");
    expect(formatSpeed(0)).toBe("");
    expect(formatSpeed(undefined)).toBe("");
  });
});
