import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { OmbDevicesSettings } from "./devices-settings";

describe("device settings", () => {
  const previousSnapshot = store.snapshot;
  let settings: OmbDevicesSettings | undefined;

  afterEach(() => {
    settings?.remove();
    settings = undefined;
    store.snapshot = previousSnapshot;
    vi.restoreAllMocks();
  });

  it("defaults mounted-first ordering on and persists changes", async () => {
    const snapshot = demoSnapshot();
    delete snapshot.settings.show_mounted_devices_first;
    store.snapshot = snapshot;
    vi.spyOn(store.backend, "saveSettings").mockResolvedValue();
    settings = new OmbDevicesSettings();
    document.body.append(settings);
    await settings.updateComplete;

    const toggle = settings.querySelector<HTMLInputElement>('input[aria-label="Show mounted devices first"]')!;
    expect(toggle.checked).toBe(true);

    toggle.click();
    await settings.updateComplete;

    expect(store.snapshot?.settings.show_mounted_devices_first).toBe(false);
    expect(store.backend.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ show_mounted_devices_first: false }),
    );
  });
});
