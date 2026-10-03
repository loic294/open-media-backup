import type { CaptureTime, FileEntry } from "../../api/types";

export interface BrowserMedia {
  file: FileEntry;
  projectIds: string[];
}

export function captureTime(file: FileEntry): number | null {
  const time = file.capture_time;
  return time != null && Number.isFinite(time) && Number.isFinite(new Date(time).getTime()) ? time : null;
}

export function captureTimeMs(capture: CaptureTime | null | undefined): number | null {
  if (!capture || capture.utc_offset_seconds == null) return null;
  const localTime = Date.parse(`${capture.local_datetime}Z`);
  const time = localTime - capture.utc_offset_seconds * 1000;
  return Number.isFinite(localTime) && Number.isFinite(time) ? time : null;
}

/** Route pages can repeat a physical source file for destinations and overlapping projects. */
export function mergeMedia(current: readonly BrowserMedia[], incoming: readonly FileEntry[]): BrowserMedia[] {
  const byPath = new Map(
    current.map((item) => [item.file.rel_path, { file: item.file, projectIds: [...item.projectIds] }]),
  );
  for (const file of incoming) {
    const item = byPath.get(file.rel_path) ?? { file, projectIds: [] };
    if (file.project_id && !item.projectIds.includes(file.project_id)) item.projectIds.push(file.project_id);
    byPath.set(file.rel_path, item);
  }
  return [...byPath.values()].sort((a, b) => {
    const left = captureTime(a.file);
    const right = captureTime(b.file);
    if (left !== right) {
      if (left === null) return 1;
      if (right === null) return -1;
      return left - right;
    }
    return a.file.rel_path < b.file.rel_path ? -1 : a.file.rel_path > b.file.rel_path ? 1 : 0;
  });
}

/** Every selected file must have an eligible capture date; missing dates are never inferred. */
export function selectedCaptureRange(
  items: readonly BrowserMedia[],
  keys: ReadonlySet<string>,
): [number, number] | null {
  const selected = items.filter((item) => keys.has(item.file.rel_path));
  if (!selected.length || selected.length !== keys.size) return null;
  const times = selected.map((item) => captureTime(item.file));
  if (times.some((time) => time === null)) return null;
  const valid = times.filter((time): time is number => time !== null);
  return [Math.min(...valid), Math.max(...valid)];
}

export function captureLabel(file: FileEntry): string {
  const time = captureTime(file);
  return time === null ? "Capture date unavailable" : new Date(time).toISOString();
}
