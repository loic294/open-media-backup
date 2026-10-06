import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import type { SourceSafeCopyDetails } from "../../api/types";
import { store } from "../../state";
import { OmbSafeCopyDialog } from "./safe-copy-dialog";
import { OmbSourceCard } from "../workspace/source-card";
import { mockStatus } from "../../api/mock/status";

const details: SourceSafeCopyDetails = {
  source_id: "s1",
  device_id: "card1",
  device_name: "Camera A",
  required_copies: 2,
  safe_copies: 0,
  wipe_eligible: false,
  blocking_reason: "Needs Home NAS",
  editable: true,
  files: [
    {
      path: "DCIM/safe.ARW",
      state: "safe",
      safe_copies: 2,
      verified_destinations: ["SSD"],
      acknowledged_destinations: ["NAS"],
      reasons: [],
    },
    {
      path: "DCIM/pending.ARW",
      state: "unsafe",
      safe_copies: 1,
      verified_destinations: ["SSD"],
      acknowledged_destinations: [],
      reasons: ["NAS offline; reconnect to transfer"],
    },
    {
      path: "DCIM/meta.THM",
      state: "excluded",
      safe_copies: 0,
      verified_destinations: [],
      acknowledged_destinations: [],
      reasons: ["Excluded by source rules"],
    },
  ],
};

describe("safe-copy dialog", () => {
  const previousSnapshot = store.snapshot;
  const previousStatus = store.status;
  const previousDialogs = store.dialogs;
  let element: OmbSafeCopyDialog | OmbSourceCard | undefined;
  const settle = async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
    await element!.updateComplete;
    await element?.querySelector("omb-modal")?.updateComplete;
  };
  const button = (label: string) =>
    [...element!.querySelectorAll<HTMLButtonElement>("button")].find(
      (item) => item.textContent?.trim() === label,
    )!;
  async function open(response = details) {
    store.snapshot = demoSnapshot();
    vi.spyOn(store.backend, "getSourceSafeCopyDetails").mockResolvedValue(structuredClone(response));
    element = new OmbSafeCopyDialog();
    element.request = { type: "safe-copy", sourceId: "s1" };
    document.body.append(element);
    await settle();
  }
  afterEach(() => {
    element?.remove();
    store.snapshot = previousSnapshot;
    store.status = previousStatus;
    store.dialogs = previousDialogs;
    vi.restoreAllMocks();
  });

  it("opens from a native keyboard-accessible badge without selecting the card", async () => {
    store.snapshot = demoSnapshot();
    store.status = mockStatus(store.snapshot, "trip", { f1: [1, 0, 0, 0], f2: [1, 0, 0, 0] }, new Set());
    store.dialogs = [];
    const select = vi.spyOn(store, "select");
    const card = new OmbSourceCard();
    card.source = store.snapshot.sources[0];
    element = card;
    document.body.append(card);
    await card.updateComplete;
    const badge = card.querySelector<HTMLButtonElement>('[aria-label="View safe-copy details"]')!;
    expect(badge.tagName).toBe("BUTTON");
    expect(badge.disabled).toBe(false);
    badge.click();
    expect(store.dialogs).toContainEqual({ type: "safe-copy", sourceId: "s1" });
    expect(select).not.toHaveBeenCalled();
  });

  it("uses backend evidence and filters safe, unsafe and excluded files with reasons", async () => {
    await open();
    expect(store.backend.getSourceSafeCopyDetails).toHaveBeenCalledWith(
      { spaceId: "travel", projectId: "trip" },
      "s1",
    );
    expect(element!.querySelector<HTMLSelectElement>('[aria-label="Safety policy"]')!.value).toBe("trip");
    expect(element!.textContent).toContain("not byte-verified");
    expect(element!.textContent).toContain("NAS offline; reconnect to transfer");
    const filter = element!.querySelector<HTMLSelectElement>('[aria-label="File safety filter"]')!;
    filter.value = "unsafe";
    filter.dispatchEvent(new Event("change"));
    await settle();
    expect(element!.querySelectorAll("tbody tr")).toHaveLength(1);
    expect(element!.querySelector("tbody")!.textContent).toContain("pending.ARW");
    const search = element!.querySelector<HTMLInputElement>('[aria-label="Search source files"]')!;
    search.value = "missing";
    search.dispatchEvent(new Event("input"));
    await settle();
    expect(element!.textContent).toContain("No files match this filter");
  });

  it("saves rules using only source identity and refreshes persisted coverage", async () => {
    await open();
    vi.spyOn(store, "reloadSnapshot").mockResolvedValue();
    vi.spyOn(store, "refreshStatus").mockImplementation(() => undefined);
    const save = vi.spyOn(store.backend, "saveSourceSafeCopyRules").mockResolvedValue();
    const rules = [{ action: "exclude" as const, syntax: "glob" as const, pattern: "*.THM" }];
    element!
      .querySelector("omb-rules-editor")!
      .dispatchEvent(new CustomEvent("rules-change", { detail: rules }));
    await settle();
    expect(element!.textContent).toContain("Unsaved rules");
    button("Save exclusions").click();
    await settle();
    expect(save).toHaveBeenCalledWith({ spaceId: "travel", projectId: "trip" }, "s1", rules);
    expect(store.reloadSnapshot).toHaveBeenCalled();
    expect(store.backend.getSourceSafeCopyDetails).toHaveBeenCalledTimes(2);
    expect(button("Save exclusions").disabled).toBe(true);
  });

  it("keeps failures visible, supports retry and never renders remote rule controls", async () => {
    await open({ ...details, editable: false, files: [] });
    expect(element!.textContent).toContain("No source files are known");
    expect(element!.textContent).toContain("Read-only");
    expect(element!.querySelector("omb-rules-editor")).toBeNull();
    vi.mocked(store.backend.getSourceSafeCopyDetails).mockRejectedValueOnce(new Error("Catalog unavailable"));
    button("Refresh").click();
    await settle();
    expect(element!.querySelector('[role="alert"]')!.textContent).toContain("Catalog unavailable");
    expect(element!.querySelector("tbody")).toBeNull();
    button("Retry").click();
    await settle();
    expect(element!.querySelector('[role="alert"]')).toBeNull();
  });

  it("retains an unsaved draft on rejected persistence", async () => {
    await open();
    vi.spyOn(store.backend, "saveSourceSafeCopyRules").mockRejectedValue(new Error("Device is busy"));
    element!
      .querySelector("omb-rules-editor")!
      .dispatchEvent(
        new CustomEvent("rules-change", { detail: [{ action: "exclude", syntax: "glob", pattern: "*" }] }),
      );
    await settle();
    button("Save exclusions").click();
    await settle();
    expect(element!.textContent).toContain("Device is busy");
    expect(element!.textContent).toContain("Unsaved rules");
    expect(button("Save exclusions").disabled).toBe(false);
  });
});
