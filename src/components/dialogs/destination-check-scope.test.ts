import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { OmbDestinationCheckDialog } from "./destination-check-dialog";

describe("destination check scope chooser", () => {
  const previousSnapshot = store.snapshot;
  const previousDialogs = store.dialogs;
  let dialog: OmbDestinationCheckDialog;

  afterEach(() => {
    dialog?.remove();
    store.snapshot = previousSnapshot;
    store.dialogs = previousDialogs;
    vi.restoreAllMocks();
  });

  async function render() {
    store.snapshot = demoSnapshot();
    dialog = new OmbDestinationCheckDialog();
    dialog.request = {
      type: "destination-check",
      destinationId: "d2",
      context: { spaceId: "travel", projectId: "trip" },
    };
    store.dialogs = [dialog.request];
    document.body.append(dialog);
    await update();
    return vi.spyOn(store, "checkDestination").mockResolvedValue(true);
  }

  async function update() {
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
  }

  async function choose(kind: string) {
    dialog.querySelector<HTMLInputElement>(`input[value="${kind}"]`)!.click();
    await update();
  }

  function start() {
    return [...dialog.querySelectorAll("button")].find(
      (button) => button.textContent?.trim() === "Start check",
    )!;
  }

  it("offers exactly three scopes and defaults to configured sources without starting", async () => {
    const check = await render();
    const radios = [...dialog.querySelectorAll<HTMLInputElement>('input[type="radio"]')];
    expect(radios.map((r) => r.value)).toEqual(["allDestination", "configuredSources", "selectedSources"]);
    expect(radios.find((r) => r.checked)?.value).toBe("configuredSources");
    expect(check).not.toHaveBeenCalled();
    start().click();
    await Promise.resolve();
    expect(check).toHaveBeenCalledWith("d2", { kind: "configuredSources" }, dialog.request.context);
  });

  it("requires selected sources and sends only checked incoming source ids", async () => {
    const check = await render();
    await choose("selectedSources");
    expect(start().disabled).toBe(true);
    const inputs = [...dialog.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
    const incoming = store.snapshot!.flows.filter((f) => f.destination_id === "d2").map((f) => f.source_id);
    expect(inputs.map((input) => input.value)).toEqual([...new Set(incoming)]);
    inputs[1].click();
    await update();
    expect(start().disabled).toBe(false);
    start().click();
    await Promise.resolve();
    expect(check).toHaveBeenCalledWith(
      "d2",
      { kind: "selectedSources", sourceIds: [inputs[1].value] },
      dialog.request.context,
    );
  });

  it("starts a source-independent inventory only after explicit confirmation", async () => {
    const check = await render();
    await choose("allDestination");
    expect(dialog.textContent).toContain("not verified");
    expect(dialog.textContent).toContain("device root");
    expect(check).not.toHaveBeenCalled();
    start().click();
    await Promise.resolve();
    expect(check).toHaveBeenCalledWith("d2", { kind: "allDestination" }, dialog.request.context);
  });

  it.each(["cancel", "escape", "backdrop"])("closes on %s without starting jobs", async (action) => {
    const check = await render();
    const native = dialog.querySelector("dialog")!;
    if (action === "cancel") {
      [...dialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Cancel")!.click();
    } else if (action === "escape") {
      const event = new Event("cancel", { cancelable: true });
      native.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(false);
      native.close();
    } else {
      expect(dialog.querySelector(".modal-backdrop")?.getAttribute("method")).toBe("dialog");
      native.close();
    }
    expect(check).not.toHaveBeenCalled();
    expect(store.dialogs).toHaveLength(0);
  });

  it("keeps the chooser open on a failed start", async () => {
    const check = await render();
    check.mockResolvedValue(false);
    start().click();
    await Promise.resolve();
    await update();
    expect(store.dialogs).toHaveLength(1);
    expect(start().disabled).toBe(false);
  });
});
