import type { Project, ProjectGranularity } from "../api/types";

export function formatProjectTime(value: number | null | undefined, granularity: ProjectGranularity): string {
  if (value == null) return "";
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return "";
  const pad = (part: number, length = 2) => String(part).padStart(length, "0");
  const year = pad(date.getFullYear(), 4);
  if (granularity === "year") return year;
  const day = `${year}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
  return granularity === "day" ? day : `${day}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** Convert a local calendar bucket to inclusive UTC instants, using calendar arithmetic across DST. */
export function parseProjectTime(value: string, granularity: ProjectGranularity, end = false): number | null {
  if (!value) return null;
  const pattern =
    granularity === "year"
      ? /^(\d{4})$/
      : granularity === "day"
        ? /^(\d{4})-(\d{2})-(\d{2})$/
        : /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/;
  const parts = pattern.exec(value);
  if (!parts) throw new Error("Invalid project capture time");
  const [, year, month = "1", day = "1", hour = "0", minute = "0"] = parts;
  const date = new Date(0);
  date.setFullYear(Number(year), Number(month) - 1, Number(day));
  date.setHours(Number(hour), Number(minute), 0, 0);
  if (Number(year) < 1 || formatProjectTime(date.getTime(), granularity) !== value)
    throw new Error("Invalid or nonexistent local project capture time");
  if (!end) return date.getTime();
  if (granularity === "year") date.setFullYear(date.getFullYear() + 1);
  else if (granularity === "day") date.setDate(date.getDate() + 1);
  else return date.getTime() + 60_000 - 1;
  date.setHours(0, 0, 0, 0);
  return date.getTime() - 1;
}

export function localProjectRange(
  project: Project,
  granularity = project.granularity ?? "minute",
  patch: Partial<Pick<Project, "start_time" | "end_time">> = {},
): Project {
  const bounds = { ...project, ...patch };
  if (project.range_is_exact && granularity === project.granularity) return bounds;
  return {
    ...bounds,
    granularity,
    range_is_exact: true,
    start_time: parseProjectTime(formatProjectTime(bounds.start_time, granularity), granularity),
    end_time: parseProjectTime(formatProjectTime(bounds.end_time, granularity), granularity, true),
  };
}
