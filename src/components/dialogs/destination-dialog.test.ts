import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { OmbDestinationDialog } from "./destination-dialog";

describe("destination dialog app type", () => {
  const previousSnapshot = store.snapshot;
  const previousBackend = {
    pickPreviewApp: store.backend.pickPreviewApp,
    saveSettings: store.backend.saveSettings,
    saveEntity: store.backend.saveEntity,
  };
  let dialog: OmbDestinationDialog | undefined;

  afterEach(() => {
    dialog?.remove();
    store.snapshot = previousSnapshot;
    store.backend.pickPreviewApp = previousBackend.pickPreviewApp;
    store.backend.saveSettings = previousBackend.saveSettings;
    store.backend.saveEntity = previousBackend.saveEntity;
    vi.restoreAllMocks();
  });

  it("switches from folder settings to app settings", async () => {
    store.snapshot = demoSnapshot();
    dialog = new OmbDestinationDialog();
    dialog.request = { type: "destination-settings", destinationId: "d1" };
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;

    expect(dialog.textContent).toContain("Destination folder");
    [...dialog.querySelectorAll("button")].find((button) => button.textContent?.trim() === "App")!.click();
    await dialog.updateComplete;

    expect(dialog.textContent).toContain("Application");
    expect(dialog.textContent).toContain("Not set on this computer");
    expect(dialog.textContent).not.toContain("Destination folder");
    expect(dialog.textContent).not.toContain("Full-card backup folder");
    expect(dialog.textContent).toContain("File rules");
  });

  it("stores the chosen app path in local settings and the display name on the destination", async () => {
    const snapshot = demoSnapshot();
    delete snapshot.settings.app_destinations?.d4;
    store.snapshot = snapshot;
    store.backend.pickPreviewApp = vi.fn(async () => "/Applications/Capture One.app");
    store.backend.saveSettings = vi.fn(async () => undefined);
    store.backend.saveEntity = vi.fn(async () => undefined);
    dialog = new OmbDestinationDialog();
    dialog.request = { type: "destination-settings", destinationId: "d4" };
    document.body.append(dialog);
    await dialog.updateComplete;

    [...dialog.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("Choose"))!
      .click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await dialog.updateComplete;

    expect(dialog.textContent).toContain("/Applications/Capture One.app");
    expect(store.backend.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ app_destinations: { d4: "/Applications/Capture One.app" } }),
    );

    [...dialog.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("Save"))!
      .click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(store.backend.saveEntity).toHaveBeenCalledWith(
      "destination",
      expect.objectContaining({ id: "d4", app_name: "Capture One" }),
    );
  });
});
