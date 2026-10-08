import { describe, expect, it } from "vitest";
import type { Project } from "../api/types";
import { demoSnapshot } from "../api/mock/data";
import { createMockBackend } from "../api/mock/mock-backend";
import { newProject, newSource, newSpace } from "./factories";
import {
  captureBucket,
  matchingProjects,
  matchesCaptureTime,
  nextProjectColor,
  PROJECT_COLORS,
  validateProjectRanges,
  validateSourceDestination,
} from "./projects";
import { previewVars } from "../utils/template";

const time = (date: string) => Date.parse(date);
const range = (name: string, start: string, end = start) => ({
  ...newProject(newSpace("Space", 0), name),
  start_time: time(start),
  end_time: time(end),
});

describe("project default colors", () => {
  const space = newSpace("Space", 0);
  const projects = PROJECT_COLORS.map((color, index) => ({
    ...newProject(space, `Project ${index}`),
    color,
  }));

  it("excludes only the last three snapshot entries, including archived projects, case-insensitively", () => {
    const recent = [
      { ...projects[0], name: "Z", color: PROJECT_COLORS[0].toLowerCase() },
      { ...projects[1], name: "A", archived: true },
      { ...projects[2], name: "B" },
    ];
    expect(nextProjectColor([...projects, ...recent], () => 0)).toBe(PROJECT_COLORS[3].toLowerCase());
    expect(nextProjectColor(projects, () => 0)).toBe(PROJECT_COLORS[0].toLowerCase());
  });

  it("selects every remaining palette option through equal random intervals", () => {
    const remaining = PROJECT_COLORS.slice(3);
    remaining.forEach((color, index) => {
      expect(nextProjectColor(projects.slice(0, 3), () => (index + 0.5) / remaining.length)).toBe(
        color.toLowerCase(),
      );
    });
  });

  it("handles zero, one, and two prior projects without excluding extra colors", () => {
    for (let count = 0; count < 3; count++) {
      expect(nextProjectColor(projects.slice(0, count), () => 0)).toBe(PROJECT_COLORS[count].toLowerCase());
    }
  });

  it("generates a valid distinct color for exhausted, empty, or invalid palettes", () => {
    const exhausted = PROJECT_COLORS.slice(0, 3);
    const generated = nextProjectColor(projects.slice(0, 3), () => 0, exhausted);
    expect(generated).toMatch(/^#[0-9a-f]{6}$/);
    expect(exhausted.map((color) => color.toLowerCase())).not.toContain(generated);
    const generatedProjects = [0, 1, 2].reduce<Project[]>((recent) => {
      return [...recent, { ...projects[0], color: nextProjectColor(recent, () => 0, []) }];
    }, []);
    const next = nextProjectColor(generatedProjects, () => 0, ["invalid", "#123"]);
    expect(next).toMatch(/^#[0-9a-f]{6}$/);
    expect(generatedProjects.map((project) => project.color)).not.toContain(next);
  });

  it("never returns invalid data for an out-of-range random source or duplicate palette values", () => {
    for (const value of [-1, 1, 2, NaN, Infinity]) {
      expect(nextProjectColor([], () => value)).toMatch(/^#[0-9a-f]{6}$/);
    }
    expect(nextProjectColor([], () => 0.6, ["#FFFFFF", "#ffffff", "#000000"])).toBe("#000000");
  });
});

describe("project capture ranges", () => {
  it("defaults remain compatible with legacy snapshots and new factories", () => {
    const space = newSpace("Space", 0);
    expect(space.allow_project_overlap).toBe(true);
    expect(newSource(space.id, "card", 0).project_scope).toEqual({ mode: "all" });
    const project = newProject(space, "Trip");
    expect(project).toMatchObject({
      start_time: null,
      end_time: null,
      granularity: "minute",
      color: "#3b82f6",
    });
    expect(matchesCaptureTime(project, time("2026-01-01T00:00:00Z"))).toBe(false);
  });

  it("includes the entire final minute, day and year bucket", () => {
    const project = range("Trip", "2026-01-01T12:34:00Z");
    expect(matchesCaptureTime(project, time("2026-01-01T12:34:59.999Z"))).toBe(true);
    expect(matchesCaptureTime(project, time("2026-01-01T12:35:00Z"))).toBe(false);
    project.granularity = "day";
    expect(matchesCaptureTime(project, time("2026-01-01T23:59:59.999Z"))).toBe(true);
    project.granularity = "year";
    expect(matchesCaptureTime(project, time("2026-12-31T23:59:59.999Z"))).toBe(true);
    expect(matchesCaptureTime(project, time("2027-01-01T00:00:00Z"))).toBe(false);
    expect(matchesCaptureTime(project, null)).toBe(false);
    expect(captureBucket(-1)).toBe(-60000);
  });

  it("rejects overlapping inclusive and mixed-granularity ranges only when disabled", () => {
    const a = range("a", "2026-01-01T10:00:00Z", "2026-01-01T12:00:00Z");
    const b = range("b", "2026-01-01T12:00:00Z", "2026-01-01T13:00:00Z");
    expect(() => validateProjectRanges([a, b])).not.toThrow();
    expect(() => validateProjectRanges([a, b], false)).toThrow("overlap");
    expect(() => validateProjectRanges([a, { ...b, archived: true }], false)).not.toThrow();
    b.start_time = time("2026-01-01T12:01:00Z");
    expect(() => validateProjectRanges([a, b], false)).not.toThrow();
    a.granularity = "day";
    expect(() => validateProjectRanges([a, b], false)).toThrow("overlap");
    expect(() => validateProjectRanges([{ ...a, end_time: null }])).toThrow("both");
  });

  it("matches all eligible projects or just selected IDs, never missing timestamps", () => {
    const snapshot = demoSnapshot();
    const source = snapshot.sources[0];
    const a = { ...range("a", "2026-01-01T12:00:00Z"), id: "a", space_id: source.space_id };
    const b = { ...a, id: "b", name: "b" };
    snapshot.projects = [
      a,
      b,
      { ...a, id: "archived", archived: true },
      { ...a, id: "foreign", space_id: "other" },
    ];
    const captureTime = a.start_time;
    expect(matchingProjects(snapshot, source, captureTime).map((p) => p.id)).toEqual(["a", "b"]);
    source.project_scope = { mode: "selected", project_ids: ["b"] };
    expect(matchingProjects(snapshot, source, captureTime).map((p) => p.id)).toEqual(["b"]);
    source.project_scope = { mode: "selected", project_ids: ["archived"] };
    expect(matchingProjects(snapshot, source, captureTime)).toEqual([]);
    expect(matchingProjects(snapshot, source, null)).toEqual([]);
    source.project_scope = { mode: "none" };
    expect(matchingProjects(snapshot, source, captureTime)).toEqual([]);
  });

  it("blocks project variables for none and does not leak defaults into unassigned paths", () => {
    const snapshot = demoSnapshot();
    const source = { ...snapshot.sources[0], project_scope: { mode: "none" } as const };
    const destination = snapshot.destinations[0];
    const space = snapshot.spaces[0];
    expect(() => validateSourceDestination(source, destination, space, snapshot.projects)).toThrow(
      "cannot use project variables",
    );
    expect(() =>
      validateSourceDestination(
        source,
        { ...destination, path_template: "{ client }" },
        space,
        snapshot.projects,
      ),
    ).toThrow();
    expect(() =>
      validateSourceDestination(
        source,
        { ...destination, path_template: "{year}/{source_name}" },
        space,
        snapshot.projects,
      ),
    ).not.toThrow();
    expect(previewVars(space, null)).not.toHaveProperty("client");
  });

  it("mock persistence rejects overlaps and none scope before mutation", async () => {
    const backend = createMockBackend();
    const snapshot = await backend.getSnapshot();
    const project = { ...snapshot.projects[0], start_time: 0, end_time: 60000 };
    await backend.saveEntity("project", project);
    await backend.saveEntity("project", { ...project, id: "overlap" });
    await expect(
      backend.saveEntity("space", { ...snapshot.spaces[0], allow_project_overlap: false }),
    ).rejects.toThrow("overlap");
    expect((await backend.getSnapshot()).spaces[0].allow_project_overlap).toBe(true);
    await expect(
      backend.saveEntity("source", { ...snapshot.sources[0], project_scope: { mode: "none" } }),
    ).rejects.toThrow("cannot use project variables");
    expect((await backend.getSnapshot()).sources[0].project_scope).toEqual({ mode: "all" });
  });
});
