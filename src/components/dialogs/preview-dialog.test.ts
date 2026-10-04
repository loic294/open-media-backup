import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileEntry, FilePage, WorkspaceFilesRequest } from "../../api/types";
import { createMockBackend } from "../../api/mock/mock-backend";
import { store } from "../../state";
import { directoryKey, fileDirectory, summarizeDirectories } from "../../utils/transfer-tree";
import { OmbPreviewDialog } from "./preview-dialog";

function file(path: string, target: string | null = `backup/${path}`): FileEntry {
  return {
    rel_path: path,
    target_path: target,
    name: path.split("/").at(-1)!,
    size: 5,
    media: "image",
    abs_path: null,
    category: "to_transfer",
    error: null,
  };
}

async function until(condition: () => boolean) {
  for (let count = 0; count < 100; count++) {
    if (condition()) return;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  throw new Error("Preview did not settle");
}

function listing(files: FileEntry[], req: WorkspaceFilesRequest, isApp = false): FilePage {
  const filtered = files.filter((entry) => !req.filter || entry.rel_path.includes(req.filter));
  const matching = req.directory
    ? filtered.filter((entry) => directoryKey(fileDirectory(entry, isApp)) === directoryKey(req.directory!))
    : filtered;
  return {
    total: matching.length,
    total_bytes: matching.length * 5,
    items: matching.slice(req.offset, req.offset + req.limit),
    directories: summarizeDirectories(filtered, isApp),
  };
}

describe("transfer folder preview", () => {
  beforeEach(async () => {
    store.snapshot = await createMockBackend().getSnapshot();
    store.status = null;
    store.dialogs = [];
    vi.spyOn(store, "toast").mockImplementation(() => {});
  });

  afterEach(() => {
    document.body.replaceChildren();
    store.dialogs = [];
    vi.restoreAllMocks();
  });

  async function preview() {
    const element = new OmbPreviewDialog();
    element.request = { type: "preview", flowId: store.snapshot!.flows[0].id };
    document.body.append(element);
    await until(() => !!element.querySelector('ul[aria-label="Transfer folder structure"] summary'));
    return element;
  }

  async function expand(element: OmbPreviewDialog, path: string) {
    const summary = element.querySelector<HTMLElement>(`summary[title="${path}"]`);
    expect(summary).not.toBeNull();
    summary!.click();
    await element.updateComplete;
  }

  it("defaults to a complete destination tree and paginates within expanded folders", async () => {
    const files = Array.from({ length: 130 }, (_, index) =>
      file(`100/A${String(index).padStart(3, "0")}.JPG`),
    );
    files.push(file("999/last.JPG"));
    const list = vi.spyOn(store, "listWorkspaceFiles").mockImplementation(async (req) => listing(files, req));
    const element = await preview();
    expect(list).toHaveBeenCalledTimes(1);
    expect(list.mock.calls[0][0].directory).toEqual({ kind: "destination", path: "" });
    expect(element.querySelector('[aria-label="Folder structure"]')?.getAttribute("aria-pressed")).toBe(
      "true",
    );
    await expand(element, "backup");
    expect(element.querySelector('summary[title="backup/999"]')).not.toBeNull();
    expect(list).toHaveBeenCalledTimes(1);
    await expand(element, "backup/100");
    await until(() => element.textContent?.includes("A119.JPG") ?? false);
    expect(element.textContent).not.toContain("A129.JPG");
    const more = [...element.querySelectorAll("button")].find((button) =>
      button.textContent?.includes("Load 10 more files"),
    )!;
    more.click();
    await until(() => element.textContent?.includes("A129.JPG") ?? false);
    expect(list.mock.calls.at(-1)![0]).toMatchObject({
      offset: 120,
      directory: { kind: "destination", path: "backup/100" },
    });
    await expand(element, "backup/999");
    await until(() => element.textContent?.includes("last.JPG") ?? false);
    expect(element.textContent).toContain("131 files");
  });

  it("retains the flat thumbnail/list views without directory filtering", async () => {
    const list = vi
      .spyOn(store, "listWorkspaceFiles")
      .mockImplementation(async (req) => listing([file("100/A.JPG")], req));
    const element = await preview();
    element.querySelector<HTMLButtonElement>('button[title="List"]')!.click();
    await until(() => !!element.querySelector("table"));
    expect(list.mock.calls.at(-1)![0].directory).toBeUndefined();
    expect(element.textContent).toContain("backup/100/A.JPG");
    element.querySelector<HTMLButtonElement>('button[title="Thumbnails"]')!.click();
    await until(() => !!element.querySelector("omb-thumbnail"));
    expect(list.mock.calls.at(-1)![0].directory).toBeUndefined();
  });

  it("retains file exclusion actions in the tree", async () => {
    vi.spyOn(store, "listWorkspaceFiles").mockImplementation(async (req) =>
      listing([file("A.JPG", "A.JPG")], req),
    );
    const element = await preview();
    await until(() => !!element.querySelector('[title^="Source: A.JPG"]'));
    const row = element.querySelector<HTMLElement>('[title^="Source: A.JPG"]')!;
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    await element.updateComplete;
    expect(element.querySelector("[data-preview-context-menu]")?.textContent).toContain(
      "Exclude exact filename",
    );
    expect(element.querySelector("[data-preview-context-menu]")?.textContent).toContain("Exclude extension");
  });

  it("shows directory failures explicitly and retries without skipping files", async () => {
    let fail = true;
    const files = [file("100/A.JPG")];
    const list = vi.spyOn(store, "listWorkspaceFiles").mockImplementation(async (req) => {
      if (req.directory?.path === "backup/100" && fail) throw new Error("Destination disconnected");
      return listing(files, req);
    });
    const element = await preview();
    await expand(element, "backup");
    await expand(element, "backup/100");
    await until(() => element.textContent?.includes("Destination disconnected") ?? false);
    expect(store.toast).toHaveBeenCalledWith("error", expect.stringContaining("Destination disconnected"));
    fail = false;
    [...element.querySelectorAll("button")].find((button) => button.textContent === "Retry")!.click();
    await until(() => element.textContent?.includes("A.JPG") ?? false);
    expect(list.mock.calls.at(-1)![0].offset).toBe(0);
  });

  it("discards old folder pages after category changes and labels ignored source paths", async () => {
    let resolveOld!: (page: FilePage) => void;
    const files = [file("100/stale.JPG")];
    vi.spyOn(store, "listWorkspaceFiles").mockImplementation(async (req) => {
      if (req.category === "ignored") return listing([file("skipped/notes.TXT", null)], req);
      if (req.directory?.path === "backup/100")
        return new Promise((resolve) => {
          resolveOld = resolve;
        });
      return listing(files, req);
    });
    const element = await preview();
    await expand(element, "backup");
    await expand(element, "backup/100");
    await until(() => !!resolveOld);
    [...element.querySelectorAll<HTMLButtonElement>('[role="tab"]')]
      .find((button) => button.textContent?.includes("Ignored"))!
      .click();
    await until(() => element.textContent?.includes("No destination (source paths)") ?? false);
    resolveOld({ total: 1, total_bytes: 5, items: files, directories: summarizeDirectories(files) });
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(element.textContent).not.toContain("stale.JPG");
    await expand(element, "skipped");
    await until(() => element.textContent?.includes("notes.TXT") ?? false);
    expect(element.textContent).not.toContain("Destination root");
  });

  it("uses source folders for app destinations without exposing routing IDs", async () => {
    const flow = store.snapshot!.flows[0];
    const destination = store.snapshot!.destinations.find((entry) => entry.id === flow.destination_id)!;
    destination.kind = "app";
    const files = [file("100/A.JPG", "synthetic-project-id/100/A.JPG")];
    const list = vi
      .spyOn(store, "listWorkspaceFiles")
      .mockImplementation(async (req) => listing(files, req, true));
    const element = await preview();
    expect(element.textContent).toContain("Source folders (open in app)");
    expect(element.textContent).not.toContain("synthetic-project-id");
    await expand(element, "100");
    await until(() => element.textContent?.includes("A.JPG") ?? false);
    expect(element.querySelector('[title*="synthetic-project-id"]')).toBeNull();
    expect(list.mock.calls[0][0].directory).toEqual({ kind: "source", path: "" });
  });

  it("refreshes planned folders when a device name changes", async () => {
    let target = "old/100/A.JPG";
    const list = vi
      .spyOn(store, "listWorkspaceFiles")
      .mockImplementation(async (req) => listing([file("100/A.JPG", target)], req));
    const element = await preview();
    await expand(element, "old");
    await expand(element, "old/100");
    await until(() => element.textContent?.includes("A.JPG") ?? false);
    target = "renamed/100/A.JPG";
    store.snapshot!.devices[0].name = "Renamed device";
    store.dispatchEvent(new Event("change"));
    await until(() => !!element.querySelector('summary[title="renamed"]'));
    expect(element.querySelector('summary[title="old"]')).toBeNull();
    expect(list.mock.calls.at(-1)![0]).toMatchObject({
      offset: 0,
      directory: { kind: "destination", path: "" },
    });
  });

  it("resets the hierarchy and directory offsets when filtering", async () => {
    const files = [file("100/A.JPG"), file("999/B.JPG")];
    const list = vi.spyOn(store, "listWorkspaceFiles").mockImplementation(async (req) => listing(files, req));
    const element = await preview();
    await expand(element, "backup");
    await expand(element, "backup/100");
    await until(() => element.textContent?.includes("A.JPG") ?? false);
    const search = element.querySelector<HTMLInputElement>('input[type="search"]')!;
    search.value = "999";
    search.dispatchEvent(new Event("input", { bubbles: true }));
    await until(() => list.mock.calls.some(([req]) => req.filter === "999"));
    await expand(element, "backup");
    expect(element.querySelector('summary[title="backup/100"]')).toBeNull();
    await expand(element, "backup/999");
    await until(() => element.textContent?.includes("B.JPG") ?? false);
    expect(element.textContent).toContain("1 files");
    expect(list.mock.calls.at(-1)![0]).toMatchObject({
      offset: 0,
      filter: "999",
      directory: { kind: "destination", path: "backup/999" },
    });
  });
});
