import type { DirectorySummary, FileDirectory, FileEntry } from "../api/types";

export function directoryKey(directory: FileDirectory): string {
  return JSON.stringify([directory.kind, directory.path]);
}

export function parentDirectory(path: string): string {
  const separator = path.lastIndexOf("/");
  return separator < 0 ? "" : path.slice(0, separator);
}

export function fileDirectory(file: FileEntry, isApp = false): FileDirectory {
  return {
    kind: !isApp && file.target_path ? "destination" : "source",
    path: parentDirectory(!isApp && file.target_path ? file.target_path : file.rel_path),
  };
}

export function childDirectories(
  directories: readonly DirectorySummary[],
  parent: FileDirectory,
): DirectorySummary[] {
  return directories
    .filter(
      (directory) =>
        directory.kind === parent.kind &&
        directory.path !== "" &&
        parentDirectory(directory.path) === parent.path,
    )
    .sort((a, b) => a.path.localeCompare(b.path));
}

/** Summarize every route before paginating files, including directory roots. */
export function summarizeDirectories(files: readonly FileEntry[], isApp = false): DirectorySummary[] {
  const summaries = new Map<string, DirectorySummary>();
  for (const file of files) {
    const directory = fileDirectory(file, isApp);
    let path = directory.path;
    for (;;) {
      const key = directoryKey({ kind: directory.kind, path });
      const summary = summaries.get(key) ?? {
        kind: directory.kind,
        path,
        total: 0,
        total_bytes: 0,
        direct_files: 0,
      };
      summary.total++;
      summary.total_bytes += file.size;
      if (path === directory.path) summary.direct_files++;
      summaries.set(key, summary);
      if (!path) break;
      path = parentDirectory(path);
    }
  }
  return [...summaries.values()].sort((a, b) => directoryKey(a).localeCompare(directoryKey(b)));
}
