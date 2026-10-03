import { describe, expect, it } from "vitest";
import { formatUpdateProgress, parseReleaseNotes, updateProgressPercent } from "./update";

describe("update helpers", () => {
  it("formats progress and percent", () => {
    expect(updateProgressPercent(12, 48)).toBe(25);
    expect(updateProgressPercent(12, null)).toBeNull();
    expect(updateProgressPercent(12, 0)).toBeNull();
    expect(formatUpdateProgress(12_400_000, 48_000_000)).toBe("12.4 MB of 48.0 MB");
    expect(formatUpdateProgress(1200, null)).toBe("1.2 KB downloaded");
  });

  it("parses plain release notes without html", () => {
    expect(parseReleaseNotes("Intro\n\n- Faster verify\n* Better rules\n• Safer sync")).toEqual([
      { kind: "paragraph", text: "Intro" },
      { kind: "bullets", items: ["Faster verify", "Better rules", "Safer sync"] },
    ]);
    expect(parseReleaseNotes("<b>not html</b>")).toEqual([{ kind: "paragraph", text: "<b>not html</b>" }]);
    expect(parseReleaseNotes(null)).toEqual([]);
  });
});
