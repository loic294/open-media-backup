import { describe, expect, it } from "vitest";
import { newProject, newSpace } from "../state/factories";
import { matchesCaptureTime } from "../state/projects";
import { formatProjectTime, localProjectRange, parseProjectTime } from "./project-time";

describe("local project calendar boundaries", () => {
  it.each(["2026-03-08", "2026-11-01", "2026-06-15", "2026-09-06", "2026-12-31"])(
    "includes exactly the local day %s, including daylight-saving changes",
    (day) => {
      const [year, month, date] = day.split("-").map(Number);
      const start = parseProjectTime(day, "day")!;
      const end = parseProjectTime(day, "day", true)!;
      expect(start).toBe(new Date(year, month - 1, date).getTime());
      expect(end).toBe(new Date(year, month - 1, date + 1).getTime() - 1);
      const project = {
        ...newProject(newSpace("Test", 0), "Day"),
        start_time: start,
        end_time: end,
      };
      expect(matchesCaptureTime(project, start)).toBe(true);
      expect(matchesCaptureTime(project, end)).toBe(true);
      expect(matchesCaptureTime(project, start - 1)).toBe(false);
      expect(matchesCaptureTime(project, end + 1)).toBe(false);
      expect(formatProjectTime(start, "day")).toBe(day);
      expect(formatProjectTime(end, "day")).toBe(day);
    },
  );

  it("stores known UTC instants for local dates in western, eastern, fractional and UTC timezones", () => {
    const fixtures: Record<string, [string, string, number, number]> = {
      "America/Los_Angeles": ["2026-06-15T07:00:00Z", "2026-06-16T06:59:59.999Z", 23, 25],
      "Europe/Paris": ["2026-06-14T22:00:00Z", "2026-06-15T21:59:59.999Z", 24, 24],
      "Asia/Kathmandu": ["2026-06-14T18:15:00Z", "2026-06-15T18:14:59.999Z", 24, 24],
      UTC: ["2026-06-15T00:00:00Z", "2026-06-15T23:59:59.999Z", 24, 24],
    };
    const fixture = fixtures[Intl.DateTimeFormat().resolvedOptions().timeZone];
    if (!fixture) return;
    const [start, end, springHours, autumnHours] = fixture;
    expect(parseProjectTime("2026-06-15", "day")).toBe(Date.parse(start));
    expect(parseProjectTime("2026-06-15", "day", true)).toBe(Date.parse(end));
    for (const [day, hours] of [
      ["2026-03-08", springHours],
      ["2026-11-01", autumnHours],
    ] as const) {
      expect(parseProjectTime(day, "day", true)! - parseProjectTime(day, "day")! + 1).toBe(hours * 3_600_000);
    }
  });

  it("includes the final minute and full local year, including years below 100", () => {
    const start = parseProjectTime("2026-06-15T18:27", "minute")!;
    expect(parseProjectTime("2026-06-15T18:27", "minute", true)).toBe(start + 59_999);
    expect(parseProjectTime("2026", "year", true)).toBe(new Date(2027, 0, 1).getTime() - 1);
    for (const year of ["0001", "0099", "2026"]) {
      expect(formatProjectTime(parseProjectTime(year, "year"), "year")).toBe(year);
    }
  });

  it("converts legacy bounds on editing precision but preserves already exact UTC bounds", () => {
    const legacy = {
      ...newProject(newSpace("Test", 0), "Legacy"),
      start_time: new Date(2026, 5, 15, 10, 30).getTime(),
      end_time: new Date(2026, 5, 16, 11, 45).getTime(),
      range_is_exact: false,
    };
    const converted = localProjectRange(legacy, "day");
    expect(converted.range_is_exact).toBe(true);
    expect(converted.start_time).toBe(new Date(2026, 5, 15).getTime());
    expect(converted.end_time).toBe(new Date(2026, 5, 17).getTime() - 1);
    const specific = localProjectRange(converted, "minute");
    expect(specific.end_time).toBe(converted.end_time);
    expect(localProjectRange(specific)).toEqual(specific);
    const repeatedHour = { ...specific, start_time: Date.parse("2026-11-01T09:30:00Z") };
    expect(localProjectRange(repeatedHour, "minute", { end_time: null }).start_time).toBe(
      repeatedHour.start_time,
    );
  });

  it("rejects invalid dates rather than silently normalizing or clearing bounds", () => {
    for (const invalid of ["2026-02-30", "2026-13-01", "0000-01-01", "26-01-01"]) {
      expect(() => parseProjectTime(invalid, "day")).toThrow("Invalid");
    }
    expect(() => parseProjectTime("2026-06-15T25:30", "minute")).toThrow("Invalid");
    expect(parseProjectTime("", "day")).toBeNull();
    expect(formatProjectTime(null, "minute")).toBe("");
    if (Intl.DateTimeFormat().resolvedOptions().timeZone === "America/Los_Angeles") {
      expect(() => parseProjectTime("2026-03-08T02:30", "minute")).toThrow("nonexistent");
    }
  });
});
