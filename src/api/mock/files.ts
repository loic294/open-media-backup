import type { FileCategory, FileEntry, FilePage, MediaKind } from "../types";

const EXT: [string, MediaKind][] = [
  ["ARW", "raw"],
  ["ARW", "raw"],
  ["MP4", "video"],
  ["ARW", "raw"],
  ["JPG", "image"],
];

/** Deterministic fake file listing for the browser demo. */
export function mockFiles(total: number, category: FileCategory, offset: number, limit: number, filter = "", target = ""): FilePage {
  const all = Array.from({ length: total }, (_, i) => {
    const [ext, media] = EXT[i % EXT.length];
    const prefix = media === "video" ? "C" : "DSC";
    const name = `${prefix}${String(7412 + i).padStart(5, "0")}.${ext}`;
    const size = media === "video" ? 1_200_000_000 + (i % 7) * 90_000_000 : 46_000_000 + (i % 5) * 1_000_000;
    const entry: FileEntry = {
      rel_path: `100MSDCF/${name}`,
      name,
      size,
      media,
      abs_path: null,
      target_path: category === "ignored" ? null : `${target}/${name}`,
      category,
      error: category === "error" ? "hash mismatch after copy" : null,
    };
    return entry;
  }).filter((f) => !filter || f.rel_path.toLowerCase().includes(filter.toLowerCase()));
  return {
    total: all.length,
    total_bytes: all.reduce((sum, f) => sum + f.size, 0),
    items: all.slice(offset, offset + limit),
  };
}
