import { describe, expect, it } from "vitest";
import type { FileEntry } from "../api/types";
import { childDirectories, directoryKey, fileDirectory, summarizeDirectories } from "./transfer-tree";
import { mockFiles } from "../api/mock/files";

function file(rel_path: string, target_path: string | null, project_id?: string): FileEntry {
  return {
    rel_path,
    target_path,
    project_id,
    name: rel_path.split("/").at(-1)!,
    size: 5,
    category: "to_transfer",
    media: "image",
    abs_path: null,
    error: null,
  };
}

describe("transfer directory summaries", () => {
  it("preserves full target hierarchy and distinct project routes with identical filenames", () => {
    const files = [
      file("100/A.JPG", "backup/Alice/100/A.JPG", "alice"),
      file("100/A.JPG", "backup/Bob/100/A.JPG", "bob"),
      file("101/A.JPG", "backup/Alice/101/A.JPG", "alice"),
      file("ROOT.JPG", "backup/ROOT.JPG"),
    ];
    const directories = summarizeDirectories(files);
    expect(directories.find((directory) => directory.path === "")).toMatchObject({
      total: 4,
      total_bytes: 20,
    });
    expect(directories.find((directory) => directory.path === "backup")).toMatchObject({
      total: 4,
      direct_files: 1,
    });
    expect(
      childDirectories(directories, { kind: "destination", path: "backup/Alice" }).map(
        (directory) => directory.path,
      ),
    ).toEqual(["backup/Alice/100", "backup/Alice/101"]);
    expect(fileDirectory(files[0])).toEqual({ kind: "destination", path: "backup/Alice/100" });
  });

  it("groups missing targets and app routes by source hierarchy, not synthetic project IDs", () => {
    const missing = file("100/A.JPG", null);
    const app = file("100/A.JPG", "synthetic-project-id/100/A.JPG");
    expect(summarizeDirectories([missing])).toContainEqual({
      kind: "source",
      path: "100",
      total: 1,
      total_bytes: 5,
      direct_files: 1,
    });
    expect(summarizeDirectories([app], true).map((directory) => directory.path)).toEqual(["", "100"]);
    expect(directoryKey({ kind: "source", path: "" })).not.toBe(
      directoryKey({ kind: "destination", path: "" }),
    );
  });

  it("makes demo target paths source-relative and summaries independent of loaded pages", () => {
    const first = mockFiles(300, "to_transfer", 0, 120, "", "backup");
    const last = mockFiles(300, "to_transfer", 240, 120, "", "backup");
    expect(first.directories).toEqual(last.directories);
    for (const entry of first.items) expect(entry.target_path).toBe(`backup/${entry.rel_path}`);
    const directory = first.directories!.find((entry) => entry.direct_files > 0 && entry.path.includes("/"))!;
    const page = mockFiles(300, "to_transfer", 0, 2, "", "backup", [], {}, directory);
    expect(page.total).toBe(directory.direct_files);
    expect(page.items).toHaveLength(2);
    expect(page.items.every((entry) => directoryKey(fileDirectory(entry)) === directoryKey(directory))).toBe(
      true,
    );
    const filtered = mockFiles(300, "to_transfer", 0, 120, "backup/PRIVATE", "backup");
    expect(filtered.items.every((entry) => entry.target_path!.includes("backup/PRIVATE"))).toBe(true);
  });

  it("flattens demo target paths without removing the destination folder", () => {
    const flattened = mockFiles(
      30,
      "to_transfer",
      0,
      30,
      "",
      "backup",
      [],
      {},
      undefined,
      false,
      null,
      false,
    );
    const nestedFile = flattened.items.find((entry) => entry.rel_path.includes("/"))!;
    expect(nestedFile.target_path).toBe(`backup/${nestedFile.name}`);
    expect(nestedFile.target_path).not.toContain(nestedFile.rel_path.slice(0, -nestedFile.name.length));
  });
});
