import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileEntry, FilePage, MediaMetadata } from "../../api/types";
import { createMockBackend } from "../../api/mock/mock-backend";
import { store } from "../../state";
import { newFlow, newProject, newSource, newSpace } from "../../state/factories";
import {
  browserDirectory,
  captureLabel,
  captureTimeMs,
  mergeMedia,
  selectedCaptureRange,
} from "./media-browser-data";
import { OmbMediaBrowserDialog } from "./media-browser-dialog";
import { OmbMediaInspector } from "./media-inspector";
import "../workspace/source-card";

function file(path: string, time: number | null = null, projectId?: string): FileEntry {
  return {
    rel_path: path,
    abs_path: `/source/${path}`,
    name: path,
    size: 100,
    media: "image",
    capture_time: time,
    project_id: projectId,
    category: "to_transfer",
    target_path: null,
    error: null,
  };
}

function page(items: FileEntry[], total = items.length): FilePage {
  return { items, total, total_bytes: items.length * 100 };
}

const metadata: MediaMetadata = {
  media_type: "image",
  size_bytes: 100,
  capture_time: { local_datetime: "2026-10-01T12:00:00", utc_offset_seconds: null, source: "exif_original" },
  dimensions: { width: 6000, height: 4000 },
  camera: { make: "Camera maker", model: "Camera model", lens_make: null, lens_model: "Lens" },
  exposure: {
    shutter_seconds: 0.01,
    aperture_f_number: 4,
    iso: 100,
    focal_length_mm: 35,
    focal_length_35mm: null,
    compensation_ev: 0,
  },
  orientation: 1,
  video: null,
};

async function until(condition: () => boolean) {
  for (let count = 0; count < 100; count++) {
    if (condition()) return;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  throw new Error("Component did not settle");
}

describe("source browser data", () => {
  it("derives folders and direct files from recursive relative paths", () => {
    expect(
      browserDirectory(
        [
          "DCIM/100MSDCF/DSC07412.ARW",
          "DCIM/100MSDCF/DSC07413.ARW",
          "DCIM/101MSDCF/C0081.MP4",
          "PRIVATE/CLIP/C0001.MP4",
          "README.TXT",
        ],
        "",
      ),
    ).toEqual({
      currentDir: "",
      breadcrumbs: [{ label: "Card root", path: "" }],
      folders: [
        { name: "DCIM", path: "DCIM", itemCount: 3 },
        { name: "PRIVATE", path: "PRIVATE", itemCount: 1 },
      ],
      files: ["README.TXT"],
    });
    expect(
      browserDirectory(
        ["DCIM/100MSDCF/DSC07412.ARW", "DCIM/100MSDCF/DSC07413.ARW", "DCIM/101MSDCF/C0081.MP4", "README.TXT"],
        "DCIM",
      ),
    ).toEqual({
      currentDir: "DCIM",
      breadcrumbs: [
        { label: "Card root", path: "" },
        { label: "DCIM", path: "DCIM" },
      ],
      folders: [
        { name: "100MSDCF", path: "DCIM/100MSDCF", itemCount: 2 },
        { name: "101MSDCF", path: "DCIM/101MSDCF", itemCount: 1 },
      ],
      files: [],
    });
  });

  it("deduplicates physical paths, retains overlapping route projects, and stably sorts captures with unknowns last", () => {
    const items = mergeMedia(
      [],
      [
        file("unknown", null),
        file("b", 200, "p1"),
        file("a", 200),
        file("first", 0),
        file("b", 200, "p2"),
        file("invalid", NaN),
      ],
    );
    expect(items.map((item) => item.file.rel_path)).toEqual(["first", "a", "b", "invalid", "unknown"]);
    expect(items.find((item) => item.file.rel_path === "b")?.projectIds).toEqual(["p1", "p2"]);
    expect(mergeMedia(items, [file("a", 200)]).map((item) => item.file.rel_path)).toEqual(
      items.map((item) => item.file.rel_path),
    );
  });

  it("uses exact inclusive Unix ms bounds, including epoch zero, and refuses absent or invalid capture dates", () => {
    const items = mergeMedia([], [file("a", 0), file("b", 200), file("unknown"), file("invalid", Infinity)]);
    expect(selectedCaptureRange(items, new Set(["a", "b"]))).toEqual([0, 200]);
    expect(selectedCaptureRange(items, new Set(["b"]))).toEqual([200, 200]);
    for (const keys of [[], ["unknown"], ["invalid"], ["a", "unknown"], ["missing"]]) {
      expect(selectedCaptureRange(items, new Set(keys))).toBeNull();
    }
    expect(captureLabel(file("unknown"))).toBe("Capture date unavailable");
    expect(captureLabel(file("epoch", 0))).toBe("1970-01-01T00:00:00.000Z");
  });

  it("converts embedded local capture times using only a known UTC offset", () => {
    expect(
      captureTimeMs({
        local_datetime: "2026-10-01T12:00:00",
        utc_offset_seconds: 7200,
        source: "exif_original",
      }),
    ).toBe(Date.UTC(2026, 9, 1, 10));
    expect(captureTimeMs({ ...metadata.capture_time!, utc_offset_seconds: null })).toBeNull();
    expect(captureTimeMs(null)).toBeNull();
  });
});

describe("source media browser components", () => {
  beforeEach(async () => {
    store.snapshot = await createMockBackend().getSnapshot();
    const space = newSpace("Media", 0);
    const project = { ...newProject(space, "Trip"), start_time: 0, end_time: 200, color: "#123456" };
    const source = newSource(space.id, "device", 0);
    source.id = "source";
    const flow = newFlow(space.id, source.id, "destination");
    flow.id = "flow";
    store.snapshot.spaces = [space];
    store.snapshot.projects = [project];
    store.snapshot.sources = [source];
    store.snapshot.flows = [flow];
    store.snapshot.settings.active_space_id = space.id;
    store.snapshot.settings.active_project_by_space = { [space.id]: project.id };
    store.dialogs = [];
    vi.spyOn(store.backend, "thumbnail").mockResolvedValue(null);
    vi.spyOn(store.backend, "getMediaMetadata").mockResolvedValue(metadata);
    vi.spyOn(store.backend, "openMedia").mockResolvedValue();
    vi.spyOn(store, "toast").mockImplementation(() => {});
  });

  afterEach(() => {
    document.body.replaceChildren();
    store.dialogs = [];
    vi.restoreAllMocks();
  });

  async function browser() {
    const element = new OmbMediaBrowserDialog();
    element.request = { type: "media-browser", sourceId: "source" };
    document.body.append(element);
    await until(() => !!element.querySelector('button[aria-label^="Select"]'));
    return element;
  }

  it("opens Browse media from the source card without changing card selection", async () => {
    const element = document.createElement("omb-source-card");
    Object.assign(element, { source: store.snapshot!.sources[0] });
    document.body.append(element);
    await until(() => !!element.querySelector("button"));
    const select = vi.spyOn(store, "select");
    const button = [...element.querySelectorAll("button")].find((button) =>
      button.textContent?.includes("Browse media"),
    )!;
    button.click();
    expect(store.dialogs).toEqual([{ type: "media-browser", sourceId: "source" }]);
    expect(select).not.toHaveBeenCalled();
  });

  it("bounds page requests, advances raw offsets despite duplicates, and never requests unrelated source flows", async () => {
    store.snapshot!.flows.push({ ...store.snapshot!.flows[0], id: "unrelated", source_id: "other-source" });
    const list = vi
      .spyOn(store.backend, "listFiles")
      .mockImplementation(async (req) =>
        req.category === "to_transfer" ? page([file("a", 0), file("a", 0)], 3) : page([]),
      );
    const element = await browser();
    await until(() => list.mock.calls.length === 4 && !element.querySelector(".loading"));
    expect(element.querySelectorAll('button[aria-label^="Select"]')).toHaveLength(1);
    expect(
      list.mock.calls.every(([req]) => req.flowId === "flow" && req.limit === 60 && req.offset === 0),
    ).toBe(true);
    const load = [...element.querySelectorAll("button")].find((button) =>
      button.textContent?.includes("Load more"),
    )!;
    load.click();
    await until(() => list.mock.calls.length === 5);
    expect(list.mock.calls[4][0].offset).toBe(2);
  });

  it("does not eagerly walk every connected flow and surfaces page failures for retry", async () => {
    for (let index = 0; index < 10; index++) {
      store.snapshot!.flows.push({ ...store.snapshot!.flows[0], id: `flow-${index}` });
    }
    const list = vi.spyOn(store.backend, "listFiles").mockResolvedValue(page([file("a", 0)], 1000));
    const element = await browser();
    await until(() => !element.querySelector(".loading"));
    expect(list).toHaveBeenCalledTimes(4);
    list.mockRejectedValue(new Error("Source disconnected"));
    [...element.querySelectorAll("button")]
      .find((button) => button.textContent?.includes("Load more"))!
      .click();
    await until(() => element.textContent?.includes("Source disconnected") ?? false);
    expect(element.querySelector('[role="alert"]')).not.toBeNull();
    expect(store.toast).toHaveBeenCalledWith("error", expect.stringContaining("Source disconnected"));
    expect(
      [...element.querySelectorAll<HTMLButtonElement>("button")].find((button) =>
        button.textContent?.includes("Load more"),
      )!.disabled,
    ).toBe(false);
  });

  it("discards outstanding route pages when the search changes", async () => {
    let resolveOld!: (value: FilePage) => void;
    const list = vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) => {
      if (req.category !== "to_transfer") return page([]);
      if (req.filter) return page([file("filtered", 100)]);
      return new Promise((resolve) => (resolveOld = resolve));
    });
    const element = new OmbMediaBrowserDialog();
    element.request = { type: "media-browser", sourceId: "source" };
    document.body.append(element);
    await until(() => list.mock.calls.length === 4 && !!element.querySelector('input[type="search"]'));
    const input = element.querySelector<HTMLInputElement>('input[type="search"]')!;
    input.value = "filtered";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await until(() => element.textContent?.includes("filtered") ?? false);
    resolveOld(page([file("stale", 0)]));
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(element.querySelector('button[aria-label="Select stale"]')).toBeNull();
    expect(
      list.mock.calls.filter(([req]) => req.filter === "filtered").every(([req]) => req.offset === 0),
    ).toBe(true);
  });

  it("selects inclusive Shift-click ranges, shows project chips, and prefills project creation with capture bounds", async () => {
    vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) =>
      req.category === "to_transfer"
        ? page([file("c", 200), file("a", 0), file("b", 100), file("unknown")])
        : page([]),
    );
    const element = await browser();
    const tiles = [...element.querySelectorAll<HTMLButtonElement>('button[aria-label^="Select"]')];
    tiles[0].click();
    tiles[2].dispatchEvent(new MouseEvent("click", { shiftKey: true, bubbles: true }));
    await element.updateComplete;
    expect(element.querySelectorAll('[aria-pressed="true"]')).toHaveLength(3);
    expect(element.querySelector(".badge")?.textContent).toContain("Trip");
    const create = [...element.querySelectorAll<HTMLButtonElement>("button")].find((button) =>
      button.textContent?.includes("Create project from selection"),
    )!;
    expect(create.disabled).toBe(false);
    create.click();
    await until(() => store.dialogs.length > 0);
    expect(store.dialogs.at(-1)).toEqual({
      type: "project",
      projectId: null,
      start_time: 0,
      end_time: 200,
    });
    tiles[3].dispatchEvent(new KeyboardEvent("keydown", { key: " ", bubbles: true }));
    await element.updateComplete;
    expect(create.disabled).toBe(false);
    expect(element.textContent).toContain("Unknown dates are not inferred");
    await until(() =>
      vi.mocked(store.backend.getMediaMetadata).mock.calls.some(([path]) => path === "/source/unknown"),
    );
    expect(store.backend.getMediaMetadata).toHaveBeenCalledWith("/source/unknown");
  });

  it("refreshes thumbnail project tags when the project list changes", async () => {
    const capture = Date.UTC(2026, 0, 14, 9, 5);
    vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) =>
      req.category === "to_transfer" ? page([file("new-project.jpg", capture)]) : page([]),
    );
    const element = await browser();
    expect(element.textContent).not.toContain("New shoot");

    store.snapshot = {
      ...store.snapshot!,
      projects: [
        ...store.snapshot!.projects,
        {
          ...newProject(store.snapshot!.spaces[0], "New shoot"),
          start_time: capture,
          end_time: capture,
          color: "#abcdef",
        },
      ],
    };
    store.dispatchEvent(new Event("change"));
    await element.updateComplete;

    expect(element.querySelector(".badge")?.textContent).toContain("New shoot");
  });

  it("uses the first project tag color for unselected thumbnail borders", async () => {
    vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) =>
      req.category === "to_transfer" ? page([file("tagged.jpg", 0)]) : page([]),
    );
    const element = await browser();
    const tile = element.querySelector<HTMLButtonElement>('button[aria-label="Select tagged.jpg"]')!;

    expect(tile.getAttribute("style")).toContain("border-color: #123456");
  });

  it("keeps selected thumbnails distinguishable with a primary offset ring", async () => {
    vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) =>
      req.category === "to_transfer" ? page([file("selected.jpg", 0)]) : page([]),
    );
    const element = await browser();
    const tile = element.querySelector<HTMLButtonElement>('button[aria-label="Select selected.jpg"]')!;
    tile.click();
    await element.updateComplete;

    expect(tile.className).toContain("ring-4");
    expect(tile.className).toContain("ring-primary");
    expect(tile.className).toContain("ring-offset-2");
    expect(tile.getAttribute("style")).toBeNull();
  });

  it("enables project creation for selected files and reads their embedded capture times on demand", async () => {
    vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) =>
      req.category === "to_transfer" ? page([file("a"), file("b")]) : page([]),
    );
    vi.mocked(store.backend.getMediaMetadata).mockImplementation(async (path) => ({
      ...metadata,
      capture_time: {
        local_datetime: path.endsWith("/a") ? "2026-10-01T12:00:00" : "2026-10-02T12:00:00",
        utc_offset_seconds: path.endsWith("/a") ? 7200 : 3600,
        source: "exif_original",
      },
    }));
    const element = await browser();
    const [first, second] = [...element.querySelectorAll<HTMLButtonElement>('button[aria-label^="Select"]')];
    first.click();
    second.dispatchEvent(new MouseEvent("click", { ctrlKey: true, bubbles: true }));
    await element.updateComplete;

    const create = [...element.querySelectorAll<HTMLButtonElement>("button")].find((button) =>
      button.textContent?.includes("Create project from selection"),
    )!;
    expect(create.disabled).toBe(false);
    create.click();

    await until(() => store.dialogs.length > 0);
    expect(vi.mocked(store.backend.getMediaMetadata)).toHaveBeenCalledWith("/source/a");
    expect(vi.mocked(store.backend.getMediaMetadata)).toHaveBeenCalledWith("/source/b");
    expect(store.dialogs.at(-1)).toEqual({
      type: "project",
      projectId: null,
      start_time: Date.UTC(2026, 9, 1, 10),
      end_time: Date.UTC(2026, 9, 2, 11),
    });
  });

  it("keeps the action usable for missing metadata but explains when capture time or timezone is unavailable", async () => {
    vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) =>
      req.category === "to_transfer" ? page([file("unknown")]) : page([]),
    );
    const element = await browser();
    element.querySelector<HTMLButtonElement>('button[aria-label="Select unknown"]')!.click();
    await element.updateComplete;
    const create = [...element.querySelectorAll<HTMLButtonElement>("button")].find((button) =>
      button.textContent?.includes("Create project from selection"),
    )!;
    expect(create.disabled).toBe(false);
    create.click();

    await until(() => element.textContent?.includes("Unknown dates are not inferred") ?? false);
    expect(store.dialogs).toHaveLength(0);
  });

  it("ignores stale inspector metadata and explicitly displays unknown offsets", async () => {
    let resolveFirst!: (value: MediaMetadata) => void;
    vi.mocked(store.backend.getMediaMetadata).mockImplementationOnce(
      () => new Promise((resolve) => (resolveFirst = resolve)),
    );
    const element = new OmbMediaInspector();
    element.file = file("first");
    document.body.append(element);
    await until(() => vi.mocked(store.backend.getMediaMetadata).mock.calls.length === 1);
    element.file = file("second");
    await until(() => element.textContent?.includes("Camera maker") ?? false);
    resolveFirst({ ...metadata, camera: { ...metadata.camera, make: "Stale camera" } });
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(element.textContent).toContain("Unknown offset; not eligible");
    expect(element.textContent).not.toContain("Stale camera");
    expect(element.textContent).toContain("Capture date unavailable");
  });

  it("shows a media thumbnail above embedded metadata in the inspector", async () => {
    vi.mocked(store.backend.thumbnail).mockResolvedValue("blob:thumbnail");
    const element = new OmbMediaInspector();
    element.file = file("photo");
    document.body.append(element);
    await until(() => !!element.querySelector('section[aria-label="Selected media preview"] img'));
    const preview = element.querySelector('section[aria-label="Selected media preview"]')!;
    const metadata = [...element.querySelectorAll("h4")].find((heading) =>
      heading.textContent?.includes("Embedded metadata"),
    )!;
    expect(preview.compareDocumentPosition(metadata) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("pins the media browser inspector while the gallery scrolls independently on large screens", async () => {
    vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) =>
      req.category === "to_transfer" ? page([file("photo", 0)]) : page([]),
    );
    const element = await browser();
    element.querySelector<HTMLButtonElement>('button[aria-label="Select photo"]')!.click();
    await element.updateComplete;

    const modal = element.querySelector("omb-modal")!;
    expect(modal.getAttribute("bodyClass")).toContain("lg:overflow-hidden");
    expect(element.querySelector('section[aria-label="Source media gallery"]')?.className).toContain(
      "lg:overflow-y-auto",
    );
    expect(element.querySelector('aside[aria-label="Media inspector"]')?.className).toContain("lg:flex-col");
    expect(element.querySelector("omb-media-inspector")?.className).toContain("lg:flex-1");
  });

  it("keeps inspector metadata in its own scrollable pane below the fixed preview", async () => {
    const element = new OmbMediaInspector();
    element.file = file("photo");
    document.body.append(element);
    await until(() => element.textContent?.includes("Camera maker") ?? false);

    expect(element.querySelector('section[aria-label="Selected media preview"]')?.className).toContain(
      "lg:shrink-0",
    );
    const metadataCard = [...element.querySelectorAll("section")].find((section) =>
      section.textContent?.includes("Embedded metadata"),
    )!;
    expect(metadataCard.className).toContain("lg:flex-1");
    expect(metadataCard.className).toContain("lg:overflow-y-auto");
  });

  it("shows system-specific open labels and opens the inspected file in the configured app", async () => {
    store.snapshot!.settings.preview_apps = { photos: "/Applications/Pixelmator Pro.app", videos: null };
    vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) =>
      req.category === "to_transfer" ? page([file("photo", 0)]) : page([]),
    );
    const openMedia = vi.mocked(store.backend.openMedia);
    const element = await browser();
    element.querySelector<HTMLButtonElement>('button[aria-label="Select photo"]')!.click();
    await element.updateComplete;

    const openButton = [...element.querySelectorAll<HTMLButtonElement>("button")].find((button) =>
      button.textContent?.includes("Open in Pixelmator Pro"),
    )!;
    expect(openButton).toBeTruthy();
    openButton.click();
    await until(() => openMedia.mock.calls.length === 1);
    expect(openMedia).toHaveBeenCalledWith("/source/photo");
  });

  it("double-clicks thumbnails to open files while leaving the tile selected", async () => {
    vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) =>
      req.category === "to_transfer" ? page([file("photo", 0)]) : page([]),
    );
    const openMedia = vi.mocked(store.backend.openMedia);
    const element = await browser();
    const tile = element.querySelector<HTMLButtonElement>('button[aria-label="Select photo"]')!;
    expect(tile.title).toBe("Double-click to open in Preview");
    tile.dispatchEvent(new MouseEvent("dblclick", { ctrlKey: true, bubbles: true }));
    await until(() => openMedia.mock.calls.length === 1);
    await element.updateComplete;
    expect(openMedia).toHaveBeenCalledWith("/source/photo");
    expect(tile.getAttribute("aria-pressed")).toBe("true");
  });

  it("shows folders before thumbnails and navigates with breadcrumbs and Up", async () => {
    vi.spyOn(store.backend, "listFiles").mockImplementation(async (req) =>
      req.category === "to_transfer"
        ? page([
            file("DCIM/100MSDCF/DSC07412.ARW", 0),
            file("DCIM/101MSDCF/C0081.MP4", 100),
            file("ROOT.JPG", 200),
          ])
        : page([]),
    );
    const element = await browser();
    const rootFolder = element.querySelector<HTMLButtonElement>('button[aria-label="Open folder DCIM"]')!;
    const rootFile = element.querySelector<HTMLButtonElement>('button[aria-label="Select ROOT.JPG"]')!;
    expect(rootFolder).toBeTruthy();
    expect(rootFolder.compareDocumentPosition(rootFile) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    rootFolder.click();
    await element.updateComplete;
    expect(element.querySelector('button[aria-label="Select ROOT.JPG"]')).toBeNull();
    expect(element.querySelector('button[aria-label="Open folder DCIM/100MSDCF"]')).toBeTruthy();
    expect(element.textContent).toContain("Card root");
    expect(element.textContent).toContain("DCIM");

    element.querySelector<HTMLButtonElement>('button[aria-label="Open folder DCIM/100MSDCF"]')!.click();
    await element.updateComplete;
    expect(element.querySelector('button[aria-label="Select DCIM/100MSDCF/DSC07412.ARW"]')).toBeTruthy();

    [...element.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("Up"))!
      .click();
    await element.updateComplete;
    expect(element.querySelector('button[aria-label="Open folder DCIM/101MSDCF"]')).toBeTruthy();

    [...element.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.trim() === "Card root")!
      .click();
    await element.updateComplete;
    expect(element.querySelector('button[aria-label="Select ROOT.JPG"]')).toBeTruthy();
  });
});
