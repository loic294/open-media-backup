import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { OmbSourceCard } from "./source-card";
import type { OmbCardContextMenu } from "./card-context-menu";

describe("source card manual wipe action", () => {
  beforeEach(() => {
    store.snapshot = demoSnapshot();
    store.status = null;
    store.dialogs = [];
    vi.spyOn(store, "toast").mockImplementation(() => {});
    vi.spyOn(store, "refreshStatus").mockImplementation(() => {});
  });

  afterEach(() => {
    document.body.replaceChildren();
    store.snapshot = null;
    store.status = null;
    store.dialogs = [];
    vi.restoreAllMocks();
  });

  async function openMenu() {
    const card = new OmbSourceCard();
    card.source = store.snapshot!.sources[0];
    document.body.append(card);
    await card.updateComplete;
    card
      .querySelector("article")!
      .dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 100, clientY: 100 }),
      );
    await card.updateComplete;
    const menu = card.querySelector<OmbCardContextMenu>("omb-card-context-menu")!;
    await menu.updateComplete;
    return {
      card,
      button: [...menu.querySelectorAll("button")].find((item) =>
        item.textContent?.includes("manually wiped"),
      )!,
    };
  }

  it("offers the offline/project-free action, confirms the device-wide scope and persists before success", async () => {
    store.snapshot!.projects = [];
    const persist = vi.spyOn(store.backend, "markSourceManuallyWiped").mockResolvedValue();
    const { card, button } = await openMenu();
    expect(button.disabled).toBe(false);
    button.click();
    await card.updateComplete;
    expect(card.querySelector("omb-card-context-menu")).toBeNull();
    expect(persist).not.toHaveBeenCalled();
    const dialog = store.dialogs.at(-1)!;
    expect(dialog.type).toBe("confirm");
    if (dialog.type !== "confirm") throw new Error("Missing confirmation");
    expect(dialog.message).toContain("every source");
    expect(dialog.message).toContain("No files are deleted");
    expect(dialog.message).toContain("all spaces and projects");
    await dialog.onConfirm();
    expect(persist).toHaveBeenCalledExactlyOnceWith(store.snapshot!.sources[0].id);
    expect(store.refreshStatus).toHaveBeenCalledOnce();
    expect(store.toast).toHaveBeenCalledWith("success", expect.stringContaining("Safe-copy checks reset"));
  });

  it("reports persistence failure without success or an optimistic reset", async () => {
    vi.spyOn(store.backend, "markSourceManuallyWiped").mockRejectedValue(new Error("catalog write failed"));
    vi.spyOn(console, "error").mockImplementation(() => {});
    const snapshot = structuredClone(store.snapshot);
    const { button } = await openMenu();
    button.click();
    const dialog = store.dialogs.at(-1)!;
    if (dialog.type !== "confirm") throw new Error("Missing confirmation");
    await dialog.onConfirm();
    expect(store.snapshot).toEqual(snapshot);
    expect(store.refreshStatus).not.toHaveBeenCalled();
    expect(store.toast).toHaveBeenCalledWith("error", expect.stringContaining("catalog write failed"));
    expect(vi.mocked(store.toast).mock.calls.some(([kind]) => kind === "success")).toBe(false);
  });

  it.each(["missing", "final"])("disables the action for a %s device", async (kind) => {
    const source = store.snapshot!.sources[0];
    source.device_id =
      kind === "missing" ? "" : store.snapshot!.devices.find((device) => device.role === "final")!.id;
    const { button } = await openMenu();
    expect(button.disabled).toBe(true);
    button.click();
    expect(store.dialogs).toEqual([]);
  });
});
