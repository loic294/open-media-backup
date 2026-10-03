import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockBackend } from "../../api/mock/mock-backend";
import { store } from "../../state";
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
