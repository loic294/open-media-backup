import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockBackend } from "../../api/mock/mock-backend";
import { store } from "../../state";
import { DEFAULT_CAMERA_THUMBNAIL_PATHS } from "../../utils/camera-thumbnails";
import { emptyAnalysisTotals } from "../../utils/speed-analysis";
import { OmbAppSettingsDialog } from "./app-settings-dialog";

async function until(condition: () => boolean) {
  for (let count = 0; count < 100; count++) {
    if (condition()) return;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  throw new Error("Component did not settle");
}

describe("app settings dialog", () => {
  beforeEach(async () => {
    store.snapshot = await createMockBackend().getSnapshot();
    vi.spyOn(store.backend, "saveSettings").mockResolvedValue();
    vi.spyOn(store.backend, "getSpeedAnalysis").mockResolvedValue({
      pairs: [],
      totals: emptyAnalysisTotals(),
    });
  });

  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
  });

  async function dialog() {
    const element = new OmbAppSettingsDialog();
    element.request = { type: "app-settings" };
    document.body.append(element);
    await until(() => element.textContent?.includes("Preview apps") ?? false);
    return element;
  }

  it("renders macOS system preview app defaults", async () => {
    const element = await dialog();

    expect(element.textContent).toContain("System default (Preview)");
    expect(element.textContent).toContain("System default (QuickTime Player)");
    expect(element.textContent).toContain("Used when double-clicking a thumbnail or choosing Open in app.");
  });

  it("navigates all three tabs with arrows, Home and End and uses a wider analysis modal", async () => {
    const element = await dialog();
    const general = element.querySelector<HTMLButtonElement>("#settings-tab-general")!;
    general.dispatchEvent(new KeyboardEvent("keydown", { key: "End" }));
    await element.updateComplete;
    expect(element.querySelector("#settings-tab-speed-analysis")?.getAttribute("aria-selected")).toBe("true");
    expect(element.querySelector("omb-modal")?.getAttribute("size")).toBe("xl");
    expect(document.activeElement?.id).toBe("settings-tab-speed-analysis");
    element
      .querySelector("#settings-tab-speed-analysis")!
      .dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
    await element.updateComplete;
    expect(element.querySelector("#settings-tab-general")?.getAttribute("aria-selected")).toBe("true");
    expect(element.querySelector("omb-modal")?.getAttribute("size")).toBe("lg");
    general.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft" }));
    await element.updateComplete;
    expect(element.querySelector("#settings-tab-speed-analysis")?.getAttribute("aria-selected")).toBe("true");
    element
      .querySelector("#settings-tab-speed-analysis")!
      .dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft" }));
    await element.updateComplete;
    expect(element.querySelector("#settings-tab-devices")?.getAttribute("aria-selected")).toBe("true");
    element
      .querySelector("#settings-tab-devices")!
      .dispatchEvent(new KeyboardEvent("keydown", { key: "Home" }));
    await element.updateComplete;
    expect(element.querySelector("#settings-tab-general")?.getAttribute("aria-selected")).toBe("true");
  });

  it("defaults sleep prevention to enabled on older snapshots and persists the toggle", async () => {
    delete store.snapshot!.settings.keep_awake_during_transfers;
    const element = await dialog();
    const toggle = element.querySelector<HTMLInputElement>('[aria-describedby="keep-awake-description"]')!;
    expect(toggle.checked).toBe(true);
    toggle.click();
    await until(() => store.snapshot?.settings.keep_awake_during_transfers === false);
    expect(store.backend.saveSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ keep_awake_during_transfers: false }),
    );
    await element.updateComplete;
    toggle.click();
    await until(() => store.snapshot?.settings.keep_awake_during_transfers === true);
    expect(store.backend.saveSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ keep_awake_during_transfers: true }),
    );
  });

  it("edits, adds, removes and resets camera thumbnail paths", async () => {
    delete store.snapshot!.settings.camera_thumbnail_paths;
    const element = await dialog();
    const inputs = () => [...element.querySelectorAll<HTMLInputElement>('input[aria-label^="Camera thumbnail path"]')];
    const button = (label: string) =>
      [...element.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.includes(label))!;
    expect(inputs().map((input) => input.value)).toEqual(DEFAULT_CAMERA_THUMBNAIL_PATHS);

    inputs()[3].value = " {stem}.JPG ";
    inputs()[3].dispatchEvent(new Event("change"));
    await until(() => store.snapshot?.settings.camera_thumbnail_paths?.[3] === "{stem}.JPG");

    await element.updateComplete;
    button("Add path").click();
    await until(() => store.snapshot?.settings.camera_thumbnail_paths?.length === 5);

    await element.updateComplete;
    element.querySelector<HTMLButtonElement>('[aria-label="Remove camera thumbnail path 1"]')!.click();
    await until(() => store.snapshot?.settings.camera_thumbnail_paths?.length === 4);
    expect(store.snapshot!.settings.camera_thumbnail_paths![0]).toBe(DEFAULT_CAMERA_THUMBNAIL_PATHS[1]);

    await element.updateComplete;
    button("Reset to defaults").click();
    await until(() => store.snapshot?.settings.camera_thumbnail_paths?.[0] === DEFAULT_CAMERA_THUMBNAIL_PATHS[0]);
    expect(store.backend.saveSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ camera_thumbnail_paths: DEFAULT_CAMERA_THUMBNAIL_PATHS }),
    );
  });

  it("renders sleep prevention as disabled when saved off", async () => {
    store.snapshot!.settings.keep_awake_during_transfers = false;
    const element = await dialog();
    expect(
      element.querySelector<HTMLInputElement>('[aria-describedby="keep-awake-description"]')!.checked,
    ).toBe(false);
  });

  it("persists a custom preview app and can reset it to the system default", async () => {
    vi.spyOn(store.backend, "pickPreviewApp").mockResolvedValue("/Applications/Preview.app");
    const element = await dialog();
    [...element.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("Choose app"))!
      .click();
    await until(() => store.snapshot?.settings.preview_apps?.photos === "/Applications/Preview.app");
    expect(store.backend.saveSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ preview_apps: { photos: "/Applications/Preview.app", videos: null } }),
    );

    await element.updateComplete;
    [...element.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("Use default"))!
      .click();
    await until(() => store.snapshot?.settings.preview_apps?.photos === null);
    expect(store.backend.saveSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ preview_apps: { photos: null, videos: null } }),
    );
  });
});
