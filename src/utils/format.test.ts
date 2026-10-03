import { describe, expect, it } from "vitest";
import { formatAgo, formatBytes, formatEta, initials, percent, plural } from "./format";

describe("format", () => {
  it("formats bytes with decimal units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(48_200_000_000)).toBe("48.2 GB");
    expect(formatBytes(1_200_000_000_000)).toBe("1.2 TB");
    expect(formatBytes(null)).toBe("—");
  });
  it("formats counts, eta, ago, initials", () => {
    expect(plural(1248, "file")).toBe("1,248 files");
    expect(plural(1, "file")).toBe("1 file");
    expect(formatEta(360)).toBe("~6 min");
    expect(formatEta(0)).toBe("");
    expect(formatAgo(Date.now() - 3 * 86_400_000)).toBe("3 days ago");
    expect(initials("Studio PC")).toBe("SP");
    expect(initials("nas")).toBe("NA");
    expect(percent(1, 3)).toBe(33);
  });
});
