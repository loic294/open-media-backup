import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { OmbWipeDialog } from "./wipe-dialog";

describe("wipe verification choice", () => {
  const previousSnapshot = store.snapshot;
  const previousDialogs = store.dialogs;
  let dialog: OmbWipeDialog;

  afterEach(() => {
    dialog?.remove();
    store.snapshot = previousSnapshot;
    store.dialogs = previousDialogs;
    vi.restoreAllMocks();
  });

  async function update() {
    await new Promise((resolve) => setTimeout(resolve, 0));
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")?.updateComplete;
  }

  async function open(eligible = true) {
    store.snapshot = demoSnapshot();
    vi.spyOn(store.backend, "planWipe").mockResolvedValue({
      source_id: "s1",
      files_total: 2,
      ignored: 0,
      copies: [],
      eligible,
      reason: eligible ? null : "Needs more copies",
    });
    const wipe = vi.spyOn(store.backend, "wipe").mockResolvedValue();
    vi.spyOn(store, "toast");
    dialog = new OmbWipeDialog();
    dialog.request = { type: "wipe-card", sourceId: "s1" };
    store.dialogs = [dialog.request];
    document.body.append(dialog);
    await update();
    return wipe;
  }

  function checkboxes() {
    return [...dialog.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
  }

  function start() {
    return dialog.querySelector<HTMLButtonElement>("button.btn-error")!;
  }

  it("defaults to verification and requires separate irreversible-action confirmation", async () => {
    const wipe = await open();
    const [verify, confirm] = checkboxes();
    expect(verify.checked).toBe(true);
    expect(confirm.checked).toBe(false);
    expect(start().disabled).toBe(true);
    expect(wipe).not.toHaveBeenCalled();
    confirm.click();
    await update();
    start().click();
    await update();
    expect(wipe).toHaveBeenCalledWith("s1", "delete_files", true);
    expect(store.toast).toHaveBeenCalledWith("success", expect.stringContaining("re-verified"));
  });

  it.each(["delete_files", "quick_format"])("allows skipping the hash pass for %s", async (method) => {
    const wipe = await open();
    if (method === "quick_format") {
      dialog.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1].click();
      await update();
    }
    const [verify, confirm] = checkboxes();
    verify.click();
    await update();
    expect(dialog.textContent).toContain("Changes or corruption since backup may go undetected");
    expect(dialog.textContent).not.toContain("If anything changed, nothing is wiped.");
    expect(start().disabled).toBe(true);
    confirm.click();
    await update();
    start().click();
    await update();
    expect(wipe).toHaveBeenCalledWith("s1", method, false);
    expect(store.toast).toHaveBeenCalledWith("success", expect.stringContaining("without re-verifying"));
  });

  it("does not offer a bypass when the device is not eligible", async () => {
    const wipe = await open(false);
    expect(checkboxes()).toHaveLength(0);
    expect(start().disabled).toBe(true);
    expect(dialog.textContent).toContain("Needs more copies");
    expect(wipe).not.toHaveBeenCalled();
  });

  it("resets verification to enabled when a new modal opens", async () => {
    await open();
    checkboxes()[0].click();
    await update();
    expect(checkboxes()[0].checked).toBe(false);
    dialog.remove();
    await open();
    expect(checkboxes()[0].checked).toBe(true);
  });

  it("keeps the modal open and reports a failed start", async () => {
    const wipe = await open();
    wipe.mockRejectedValue(new Error("Device disconnected"));
    checkboxes()[1].click();
    await update();
    start().click();
    await update();
    expect(store.dialogs).toHaveLength(1);
    expect(store.toast).toHaveBeenCalledWith("error", "Error: Device disconnected");
    expect(start().disabled).toBe(false);
  });
});
