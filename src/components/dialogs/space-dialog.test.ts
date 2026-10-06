import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { OmbSpaceDialog } from "./space-dialog";

describe("space skipped-duplicate setting", () => {
  const previousSnapshot = store.snapshot;
  const previousSaveEntity = store.backend.saveEntity;
  let dialog: OmbSpaceDialog | undefined;

  afterEach(() => {
    dialog?.remove();
    store.snapshot = previousSnapshot;
    store.backend.saveEntity = previousSaveEntity;
    vi.restoreAllMocks();
  });

  it("defaults off for old snapshots and sync-saves the explicit choice", async () => {
    const snapshot = demoSnapshot();
    delete snapshot.spaces[0].skip_counts_as_safe_copy;
    store.snapshot = snapshot;
    store.backend.saveEntity = vi.fn(async () => undefined);
    dialog = new OmbSpaceDialog();
    dialog.request = { type: "space-settings", spaceId: "travel" };
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;

    const toggle = [...dialog.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].find((input) =>
      input.parentElement?.textContent?.includes("Count deliberate “Skip” decisions as safe copies"),
    )!;
    expect(toggle.checked).toBe(false);
    expect(dialog.textContent).toContain("matching hashes");
    toggle.click();
    await dialog.updateComplete;
    [...dialog.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.trim() === "Save")!
      .click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(store.backend.saveEntity).toHaveBeenCalledWith(
      "space",
      expect.objectContaining({ id: "travel", skip_counts_as_safe_copy: true }),
    );
  });

  it("keeps the temporary-copies field constrained to the modal width", async () => {
    store.snapshot = demoSnapshot();
    dialog = new OmbSpaceDialog();
    dialog.request = { type: "space-settings", spaceId: "travel" };
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;

    const fieldset = dialog.querySelector<HTMLFieldSetElement>("#omb-space-temp-copies")!;
    expect(fieldset.classList).toContain("min-w-0");
    expect(fieldset.querySelector("select")!.classList).toContain("w-full");
    const hint = fieldset.querySelector<HTMLParagraphElement>("p.label")!;
    expect(hint.classList).toContain("whitespace-normal");
  });

  it("owns and saves the copy threshold, rejecting invalid counts before saving", async () => {
    store.snapshot = demoSnapshot();
    store.backend.saveEntity = vi.fn(async () => undefined);
    dialog = new OmbSpaceDialog();
    dialog.request = { type: "space-settings", spaceId: "travel" };
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
    const input = dialog.querySelector<HTMLInputElement>(
      '[aria-label="Required effective copies before wiping"]',
    )!;
    const save = [...dialog.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent?.trim() === "Save",
    )!;
    expect(input.value).toBe("2");
    expect(dialog.textContent?.replace(/\s+/g, " ")).toContain("highest previous project requirement");
    input.value = "0";
    input.dispatchEvent(new Event("input"));
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
    expect(save.disabled).toBe(true);
    input.value = "4";
    input.dispatchEvent(new Event("input"));
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
    save.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(store.backend.saveEntity).toHaveBeenCalledWith(
      "space",
      expect.objectContaining({ final_copies_required: 4 }),
    );
  });
});
