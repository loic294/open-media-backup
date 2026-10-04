import type { FileCategory, FileDirectory, FileEntry, FilePage, MediaKind, FileRule } from "../types";
import { rulesAllowPath } from "../../utils/file-rules";
import { directoryKey, fileDirectory, summarizeDirectories } from "../../utils/transfer-tree";

const EXT: [string, MediaKind][] = [
  ["ARW", "raw"],
  ["ARW", "raw"],
  ["MP4", "video"],
  ["ARW", "raw"],
  ["JPG", "image"],
];
const DEMO_FOLDERS = ["DCIM/100MSDCF", "DCIM/101MSDCF", "PRIVATE/CLIPS", "DCIM/102MSDCF"];

/** Deterministic fake file listing for the browser demo. */
export function mockFiles(
  total: number,
  category: FileCategory,
  offset: number,
  limit: number,
  filter = "",
  target = "",
  rules: FileRule[] = [],
  vars: Record<string, string> = {},
  directory?: FileDirectory,
  isApp = false,
  configError?: string | null,
  preserveFileStructure = true,
): FilePage {
  const all = Array.from({ length: total }, (_, i) => {
    const [ext, media] = EXT[i % EXT.length];
    const prefix = media === "video" ? "C" : "IMG_";
    const name = `${prefix}${String(7412 + i).padStart(5, "0")}.${ext}`;
    const size = media === "video" ? 1_200_000_000 + (i % 7) * 90_000_000 : 46_000_000 + (i % 5) * 1_000_000;
    const folder = i % 17 === 0 ? "" : DEMO_FOLDERS[Math.floor(i / 12) % DEMO_FOLDERS.length];
    const entry: FileEntry = {
      rel_path: folder ? `${folder}/${name}` : name,
      name,
      size,
      media,
      abs_path: null,
      target_path:
        category === "ignored" || configError
          ? null
          : [target, ...(preserveFileStructure ? [folder] : []), name].filter(Boolean).join("/"),
      category,
      error: configError ?? (category === "error" ? "hash mismatch after copy" : null),
      capture_time: Date.UTC(2026, 0, 14, 9, (i * 7) % 60),
    };
    return entry;
  }).filter(
    (f) =>
      (!filter ||
        f.rel_path.toLowerCase().includes(filter.toLowerCase()) ||
        f.target_path?.toLowerCase().includes(filter.toLowerCase())) &&
      (category === "ignored" || rulesAllowPath(rules, f.rel_path, vars)),
  );
  const matching = directory
    ? all.filter((file) => directoryKey(fileDirectory(file, isApp)) === directoryKey(directory))
    : all;
  return {
    total: matching.length,
    total_bytes: matching.reduce((sum, f) => sum + f.size, 0),
    items: matching.slice(offset, offset + Math.min(1000, Math.max(1, limit))),
    directories: summarizeDirectories(all, isApp),
  };
}
