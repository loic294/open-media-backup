import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import type { OmbTemplateInput } from "../form/template-input";
import { OmbDestinationDialog } from "./destination-dialog";
import { OmbSourceDialog } from "./source-dialog";

describe("task naming settings", () => {
  const previousSnapshot = store.snapshot;
  const previousDialogs = store.dialogs;
  let dialog: OmbSourceDialog | OmbDestinationDialog | undefined;

  afterEach(() => {
    dialog?.remove();
    store.snapshot = previousSnapshot;
    store.dialogs = previousDialogs;
    vi.restoreAllMocks();
  });

  async function renderSource() {
    store.snapshot = demoSnapshot();
    dialog = new OmbSourceDialog();
    dialog.request = { type: "source-settings", sourceId: "s1" };
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
    return dialog;
  }

  async function renderDestination(id: string) {
    store.snapshot = demoSnapshot();
    dialog = new OmbDestinationDialog();
    dialog.request = { type: "destination-settings", destinationId: id };
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
    return dialog;
  }

  function input(name: string, value: string) {
    const field = dialog!.querySelector<HTMLInputElement>(`input[aria-label="${name}"]`)!;
    field.value = value;
    field.dispatchEvent(new Event("input", { bubbles: true }));
  }

  it("shows separate source names and saves trimmed overrides without renaming hardware", async () => {
    const sourceDialog = await renderSource();
    const devices = structuredClone(store.snapshot!.devices);
    expect(
      sourceDialog.querySelector<HTMLInputElement>('input[aria-label="Source task name"]')!.placeholder,
    ).toBe("Camera A · Card 1");
    expect(
      sourceDialog.querySelector<HTMLInputElement>('input[aria-label="Device name for backup"]')!.placeholder,
    ).toBe("Camera A · Card 1");
    input("Source task name", " Photo ingest ");
    await sourceDialog.updateComplete;
    input("Device name for backup", " Camera A ");
    await sourceDialog.updateComplete;
    await sourceDialog.querySelector("omb-modal")!.updateComplete;
    expect(sourceDialog.querySelector("omb-modal")!.heading).toBe("Source · Photo ingest");
    expect(sourceDialog.querySelector<OmbTemplateInput>("omb-template-input")!.vars.source_name).toBe(
      "Camera A",
    );
    const save = vi.spyOn(store, "save").mockResolvedValue(true);
    [...sourceDialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Save")!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(save).toHaveBeenCalledWith(
      "source",
      expect.objectContaining({
        id: "s1",
        task_name: "Photo ingest",
        backup_name: "Camera A",
        device_id: "card1",
      }),
    );
    expect(store.snapshot!.devices).toEqual(devices);
  });

  it.each(["d1", "d4"])("saves a display-only name for destination %s", async (id) => {
    const destinationDialog = await renderDestination(id);
    const original = structuredClone(store.snapshot!.destinations.find((d) => d.id === id)!);
    input("Destination task name", " Photo archive ");
    await destinationDialog.updateComplete;
    await destinationDialog.querySelector("omb-modal")!.updateComplete;
    expect(destinationDialog.querySelector("omb-modal")!.heading).toBe("Destination · Photo archive");
    const save = vi.spyOn(store, "save").mockResolvedValue(true);
    [...destinationDialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Save")!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(save).toHaveBeenCalledWith("destination", { ...original, task_name: "Photo archive" });
  });

  it("uses the source task name in removal confirmation, leaving backup and device names separate", async () => {
    const sourceDialog = await renderSource();
    input("Source task name", "Only these photos");
    await sourceDialog.updateComplete;
    await sourceDialog.querySelector("omb-modal")!.updateComplete;
    const open = vi.spyOn(store, "open");
    [...sourceDialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Remove")!.click();
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ title: "Remove Only these photos?" }));
  });
});
