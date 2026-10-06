import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockBackend } from "../../api/mock/mock-backend";
import { store } from "../../state";
import { OmbAppSettingsDialog } from "./app-settings-dialog";
import { OmbDeviceDialog } from "./device-dialog";

async function until(condition: () => boolean) {
  for (let i = 0; i < 100; i++) {
    if (condition()) return;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  throw new Error("Component did not settle");
}

describe("device settings", () => {
  let backend: ReturnType<typeof createMockBackend>;
  const previousSnapshot = store.snapshot;
  const previousDialogs = store.dialogs;

  beforeEach(async () => {
    backend = createMockBackend();
    store.snapshot = await backend.getSnapshot();
    store.volumes = await backend.listVolumes();
    store.dialogs = [];
    vi.spyOn(store.backend, "getSnapshot").mockImplementation(backend.getSnapshot);
    vi.spyOn(store.backend, "saveEntity").mockImplementation(backend.saveEntity);
    vi.spyOn(store.backend, "registerDevice").mockImplementation(backend.registerDevice);
    vi.spyOn(store.backend, "relinkDevice").mockImplementation(backend.relinkDevice);
    vi.spyOn(store.backend, "deleteEntity").mockImplementation(backend.deleteEntity);
    vi.spyOn(store.backend, "listVolumes").mockImplementation(backend.listVolumes);
    vi.spyOn(store, "refreshStatus").mockImplementation(() => undefined);
    vi.spyOn(store, "toast").mockImplementation(() => undefined);
  });

  afterEach(() => {
    document.body.replaceChildren();
    store.snapshot = previousSnapshot;
    store.dialogs = previousDialogs;
    store.volumes = [];
    vi.restoreAllMocks();
  });

  function button(root: Element, label: string) {
    return [...root.querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent?.trim() === label,
    )!;
  }

  async function editor(deviceId: string | null = null) {
    const element = new OmbDeviceDialog();
    element.request = { type: "device-settings", deviceId };
    document.body.append(element);
    await until(() => !!element.querySelector('input[aria-label="Device name"]'));
    return element;
  }

  async function input(element: OmbDeviceDialog, label: string, value: string) {
    const field = element.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;
    field.value = value;
    field.dispatchEvent(new Event("input", { bubbles: true }));
    await element.updateComplete;
    await element.querySelector("omb-modal")!.updateComplete;
  }

  async function devicesTab() {
    const settings = new OmbAppSettingsDialog();
    settings.request = { type: "app-settings" };
    document.body.append(settings);
    await until(() => !!settings.querySelector('[role="tab"]'));
    button(settings, "Devices").click();
    await until(
      () =>
        !!settings.querySelector("omb-devices-settings li") ||
        !!settings.textContent?.includes("No devices yet"),
    );
    return settings;
  }

  it("lists every device, including remote/offline devices, and preserves General settings", async () => {
    const settings = await devicesTab();
    expect(settings.querySelectorAll("[data-device-id]")).toHaveLength(store.snapshot!.devices.length);
    for (const device of store.snapshot!.devices) expect(settings.textContent).toContain(device.name);
    expect(settings.textContent).toContain("Not located on this computer");
    expect(settings.querySelector('[aria-selected="true"]')?.textContent?.trim()).toBe("Devices");
    const card = settings.querySelector('[data-device-id="card1"]')!;
    expect(card.textContent).toContain("1 source(s)");
    button(card, "Edit").click();
    expect(store.dialogs.at(-1)).toEqual({ type: "device-settings", deviceId: "card1" });
    button(settings, "General").click();
    await until(() => !!settings.textContent?.includes("Preview apps"));
    expect(settings.textContent).toContain("Appearance");
  });

  it("offers add and an empty state", async () => {
    store.snapshot = { ...store.snapshot!, devices: [] };
    const settings = await devicesTab();
    expect(settings.textContent).toContain("No devices yet");
    button(settings, "Add device").click();
    expect(store.dialogs.at(-1)).toEqual({ type: "device-settings", deviceId: null });
  });

  it("supports keyboard navigation between settings tabs", async () => {
    const settings = await devicesTab();
    const devices = button(settings, "Devices");
    devices.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }));
    await until(() => settings.querySelector('[aria-selected="true"]')?.textContent?.trim() === "General");
    expect(document.activeElement).toBe(button(settings, "General"));
    expect(button(settings, "Devices").tabIndex).toBe(-1);
    button(settings, "General").dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true }));
    await until(
      () => settings.querySelector('[aria-selected="true"]')?.textContent?.trim() === "Speed Analysis",
    );
    expect(document.activeElement).toBe(button(settings, "Speed Analysis"));
  });

  it("confirms removal with usage and preserves tasks, connections and history", async () => {
    const settings = await devicesTab();
    button(settings.querySelector('[data-device-id="card1"]')!, "Remove").click();
    const request = store.dialogs.at(-1)!;
    expect(request.type).toBe("confirm");
    if (request.type !== "confirm") throw new Error("Expected confirmation");
    expect(request.message).toContain("1 source(s)");
    expect(request.message).toContain("Physical files and backup history are not deleted");
    expect(store.backend.deleteEntity).not.toHaveBeenCalled();
    const before = store.snapshot!;
    await request.onConfirm();
    expect(store.snapshot!.sources).toHaveLength(before.sources.length);
    expect(store.snapshot!.sources.find((s) => s.id === "s1")!.device_id).toBe("");
    expect(store.snapshot!.flows).toEqual(before.flows);
    await until(() => !settings.querySelector('[data-device-id="card1"]'));
  });

  it("canceling removal does not change devices or assignments", async () => {
    const settings = await devicesTab();
    const before = store.snapshot;
    button(settings.querySelector('[data-device-id="card1"]')!, "Remove").click();
    store.close(store.dialogs.at(-1));
    expect(store.snapshot).toBe(before);
    expect(store.backend.deleteEntity).not.toHaveBeenCalled();
  });

  it("keeps the device and assignments visible if deletion fails", async () => {
    vi.spyOn(store.backend, "deleteEntity").mockRejectedValue(new Error("database locked"));
    const before = store.snapshot;
    expect(await store.removeDevice("card1")).toBe(false);
    expect(store.snapshot).toBe(before);
    expect(store.toast).toHaveBeenCalledWith("error", expect.stringContaining("database locked"));
    expect(store.backend.listVolumes).not.toHaveBeenCalled();
  });

  it("requires a nonblank name and saves offline devices without a mapping", async () => {
    const element = await editor();
    expect(button(element, "Add device").disabled).toBe(true);
    await input(element, "Device name", "   ");
    expect(button(element, "Add device").disabled).toBe(true);
    await input(element, "Device name", " Offline archive ");
    button(element, "Add device").click();
    await until(() => store.snapshot!.devices.some((d) => d.name === "Offline archive"));
    expect(store.backend.registerDevice).not.toHaveBeenCalled();
    const device = store.snapshot!.devices.find((d) => d.name === "Offline archive")!;
    expect(store.snapshot!.mappings.some((m) => m.device_id === device.id)).toBe(false);
  });

  it("edits shared metadata while preserving identity and task names", async () => {
    const before = structuredClone(store.snapshot!);
    const element = await editor("card1");
    expect(element.textContent).toContain("Detected hardware (read-only)");
    await input(element, "Device name", " Camera card renamed ");
    await input(element, "Device description", "Physical card");
    const role = element.querySelector<HTMLSelectElement>('select[aria-label="Device role"]')!;
    role.value = "temporary";
    role.dispatchEvent(new Event("change", { bubbles: true }));
    await element.updateComplete;
    await element.querySelector("omb-modal")!.updateComplete;
    button(element, "Save device").click();
    await until(() => store.snapshot!.devices.find((d) => d.id === "card1")!.name === "Camera card renamed");
    const device = store.snapshot!.devices.find((d) => d.id === "card1")!;
    expect(device).toEqual({
      ...before.devices.find((d) => d.id === "card1")!,
      name: "Camera card renamed",
      description: "Physical card",
      role: "temporary",
    });
    expect(store.snapshot!.sources).toEqual(before.sources);
    expect(store.snapshot!.destinations).toEqual(before.destinations);
    expect(store.snapshot!.mappings).toEqual(before.mappings);
  });

  it("registers a device on a selected volume and refreshes volume associations", async () => {
    const element = await editor();
    await input(element, "Device name", "New card");
    element.querySelector<HTMLInputElement>('input[type="radio"]')!.dispatchEvent(new Event("change"));
    await element.updateComplete;
    await element.querySelector("omb-modal")!.updateComplete;
    button(element, "Add device").click();
    await until(() =>
      store.volumes.some((v) => v.device_id !== null && v.mount_path === "/Volumes/Untitled"),
    );
    expect(store.backend.registerDevice).toHaveBeenCalledWith(
      "/Volumes/Untitled",
      expect.objectContaining({ name: "New card" }),
    );
  });

  it("locates an offline device on this computer and preserves remote mappings", async () => {
    vi.spyOn(store.backend, "pickFolder").mockResolvedValue("/Volumes/Untitled");
    const before = store.snapshot!.mappings.filter((m) => m.device_id === "hdd");
    const element = await editor("hdd");
    expect(element.textContent).toContain("On ");
    button(element, "Choose folder...").click();
    await until(() => !!element.textContent?.includes("/Volumes/Untitled"));
    button(element, "Save device").click();
    await until(() =>
      store.snapshot!.mappings.some(
        (m) => m.device_id === "hdd" && m.computer_id === store.snapshot!.computer.id,
      ),
    );
    expect(store.backend.relinkDevice).toHaveBeenCalledWith("hdd", "/Volumes/Untitled");
    expect(store.snapshot!.mappings).toEqual(expect.arrayContaining(before));
  });

  it("canceling device edits does not save metadata or location", async () => {
    const element = await editor("card1");
    const before = store.snapshot;
    await input(element, "Device name", "Do not save");
    button(element, "Cancel").click();
    expect(store.snapshot).toBe(before);
    expect(store.backend.saveEntity).not.toHaveBeenCalled();
    expect(store.backend.relinkDevice).not.toHaveBeenCalled();
  });

  it("reports save failures and keeps the editor open", async () => {
    vi.spyOn(store.backend, "saveEntity").mockRejectedValue(new Error("disk full"));
    const element = await editor();
    const close = vi.spyOn(element.querySelector("omb-modal")!, "close");
    await input(element, "Device name", "Not saved");
    button(element, "Add device").click();
    await until(() => vi.mocked(store.toast).mock.calls.length > 0);
    expect(store.toast).toHaveBeenCalledWith("error", expect.stringContaining("disk full"));
    expect(close).not.toHaveBeenCalled();
    expect(store.snapshot!.devices.some((d) => d.name === "Not saved")).toBe(false);
  });

  it("reports location picker errors without changing the mapping", async () => {
    vi.spyOn(store.backend, "pickFolder").mockRejectedValue(new Error("permission denied"));
    const before = store.snapshot!.mappings;
    const element = await editor("card1");
    button(element, "Choose folder...").click();
    await until(() => vi.mocked(store.toast).mock.calls.length > 0);
    expect(store.toast).toHaveBeenCalledWith("error", expect.stringContaining("permission denied"));
    expect(store.snapshot!.mappings).toBe(before);
  });

  it("does not resurrect a device removed while its editor is open", async () => {
    const element = await editor("card1");
    store.snapshot = { ...store.snapshot!, devices: store.snapshot!.devices.filter((d) => d.id !== "card1") };
    button(element, "Save device").click();
    expect(store.toast).toHaveBeenCalledWith("error", expect.stringContaining("was removed"));
    expect(store.backend.saveEntity).not.toHaveBeenCalled();
  });
});
