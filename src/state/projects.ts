import type { Destination, Project, ProjectGranularity, Snapshot, Source, Space } from "../api/types";
import { templateVars } from "../utils/template";

export const PROJECT_COLORS = [
  "#A78BFA",
  "#38BDF8",
  "#2DD4BF",
  "#34D399",
  "#A3E635",
  "#FBBF24",
  "#FB923C",
  "#F472B6",
  "#C084FC",
  "#818CF8",
  "#60A5FA",
  "#22D3EE",
  "#4ADE80",
  "#FACC15",
  "#F97316",
  "#E879F9",
] as const;

function colorForHue(hue: number): string {
  const chroma = 0.72;
  const h = hue / 60;
  const x = chroma * (1 - Math.abs((h % 2) - 1));
  const [r, g, b] =
    h < 1
      ? [chroma, x, 0]
      : h < 2
        ? [x, chroma, 0]
        : h < 3
          ? [0, chroma, x]
          : h < 4
            ? [0, x, chroma]
            : h < 5
              ? [x, 0, chroma]
              : [chroma, 0, x];
  const m = 0.58 - chroma / 2;
  return `#${[r, g, b]
    .map((channel) =>
      Math.round((channel + m) * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`;
}

/** Uses the snapshot tail (new projects are appended), never the sorted display order. */
export function nextProjectColor(
  projects: readonly Project[],
  random: () => number = Math.random,
  palette: readonly string[] = PROJECT_COLORS,
): string {
  const used = new Set(
    projects
      .slice(-3)
      .map((project) => project.color?.toLowerCase())
      .filter(Boolean),
  );
  const available = [
    ...new Set(palette.filter((color) => /^#[0-9a-f]{6}$/i.test(color)).map((color) => color.toLowerCase())),
  ].filter((color) => !used.has(color));
  if (available.length) {
    const value = random();
    const index = Number.isFinite(value)
      ? Math.floor(Math.max(0, Math.min(value, 1 - Number.EPSILON)) * available.length)
      : 0;
    return available[index];
  }
  // At most three recent colors are excluded, so the first four distinct hues suffice.
  for (let step = 0; ; step++) {
    const color = colorForHue((step * 137.508) % 360);
    if (!used.has(color.toLowerCase())) return color;
  }
}

/** UTC capture-time buckets mirror domain/project.rs. */
export function captureBucket(time: number, granularity: ProjectGranularity = "minute"): number {
  const date = new Date(time);
  if (granularity === "year") {
    date.setUTCMonth(0, 1);
    date.setUTCHours(0, 0, 0, 0);
  } else if (granularity === "day") {
    date.setUTCHours(0, 0, 0, 0);
  } else {
    date.setUTCSeconds(0, 0);
  }
  return date.getTime();
}

export function projectRange(project: Project): [number, number] | null {
  const { start_time: start, end_time: end } = project;
  if (start == null && end == null) return null;
  if (start == null || end == null) throw new Error("Project start and end times must both be set");
  const range: [number, number] = [
    captureBucket(start, project.granularity),
    captureBucket(end, project.granularity),
  ];
  if (!Number.isFinite(range[0]) || !Number.isFinite(range[1]))
    throw new Error("Invalid project capture time");
  if (range[0] > range[1]) throw new Error("Project start time must not be after end time");
  return range;
}

export function matchesCaptureTime(project: Project, time: number | null | undefined): boolean {
  if (time == null) return false;
  const range = projectRange(project);
  const bucket = captureBucket(time, project.granularity);
  return range !== null && range[0] <= bucket && bucket <= range[1];
}

export function validateProjectRanges(projects: Project[], allowOverlap = true): void {
  const ranges = projects.map(projectRange);
  if (allowOverlap) return;
  const active = projects
    .map((project, index) => ({ project, range: ranges[index] }))
    .filter(({ project }) => !project.archived);
  for (let i = 0; i < active.length; i++) {
    const { project, range } = active[i];
    if (!range) continue;
    for (let j = i + 1; j < active.length; j++) {
      const other = active[j].range;
      if (!other) continue;
      if (
        matchesCaptureTime(project, other[0]) ||
        matchesCaptureTime(project, other[1]) ||
        matchesCaptureTime(active[j].project, range[0]) ||
        matchesCaptureTime(active[j].project, range[1])
      ) {
        throw new Error(`Project ranges overlap: ${project.name} and ${active[j].project.name}`);
      }
    }
  }
}

export function matchingProjects(
  snapshot: Snapshot,
  source: Source,
  captureTime: number | null | undefined,
): Project[] {
  const scope = source.project_scope ?? { mode: "all" };
  if (scope.mode === "none") return [];
  return snapshot.projects.filter(
    (project) =>
      project.space_id === source.space_id &&
      !project.archived &&
      (scope.mode === "all" || scope.project_ids.includes(project.id)) &&
      matchesCaptureTime(project, captureTime),
  );
}

export function usesProjectVariables(
  template: string,
  space: Space,
  projects: Project[],
  marker = false,
): boolean {
  const names = new Set([
    "project",
    "project_name",
    ...space.variables.map((v) => v.name),
    ...projects.flatMap((p) => Object.keys(p.values)),
  ]);
  return templateVars(template).some((name) => !(marker && name === "backup_folder") && names.has(name));
}

export function validateSourceDestination(
  source: Source,
  destination: Destination,
  space: Space,
  projects: Project[],
): void {
  if ((destination.kind ?? "folder") === "app") return;
  if (
    source.project_scope?.mode === "none" &&
    usesProjectVariables(destination.path_template, space, projects, destination.use_backup_marker)
  ) {
    throw new Error("Source project scope is none; destination path cannot use project variables");
  }
}

export function validateProjectConfiguration(snapshot: Snapshot, spaceId: string): void {
  const space = snapshot.spaces.find((s) => s.id === spaceId);
  if (!space) throw new Error("Space not found");
  const projects = snapshot.projects.filter((p) => p.space_id === spaceId);
  validateProjectRanges(projects, space.allow_project_overlap ?? true);
  for (const source of snapshot.sources.filter((s) => s.space_id === spaceId)) {
    if (
      source.project_scope?.mode === "selected" &&
      source.project_scope.project_ids.some((id) => !projects.some((p) => p.id === id))
    ) {
      throw new Error("Source references a project outside its space or a missing project");
    }
  }
  for (const flow of snapshot.flows.filter((f) => f.space_id === spaceId)) {
    const source = snapshot.sources.find((s) => s.id === flow.source_id);
    const destination = snapshot.destinations.find((d) => d.id === flow.destination_id);
    if (source && destination) validateSourceDestination(source, destination, space, projects);
  }
}
