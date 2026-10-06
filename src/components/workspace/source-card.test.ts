import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { demoCounts, demoOffline, demoSnapshot } from "../../api/mock/data";
import { mockWorkspaceStatus } from "../../api/mock/status";
import { store } from "../../state";
import { OmbSourceCard } from "./source-card";

describe("source card status indicators", () => {
  beforeEach(() => {
    store.snapshot = demoSnapshot();
    store.status = mockWorkspaceStatus(
      store.snapshot,
      store.context!,
      structuredClone(demoCounts),
      new Set(demoOffline),
    );
    store.statusLoading = false;
    store.statusError = null;
    store.volumes = [
      {
        mount_path: "/Volumes/CAM_A_01",
        name: "CAM_A_01",
        volume_uuid: null,
        hw_serial: null,
        total_bytes: 128e9,
        free_bytes: 64e9,
        removable: true,
        device_id: "card1",
        matched_by: "mapping",
      },
    ];
    store.dialogs = [];
    vi.spyOn(store, "toast").mockImplementation(() => {});
  });

  afterEach(() => {
    document.body.replaceChildren();
    store.snapshot = null;
    store.status = null;
    store.statusLoading = false;
    store.statusError = null;
    store.volumes = [];
    store.dialogs = [];
    vi.restoreAllMocks();
  });

  async function renderCard() {
    const card = new OmbSourceCard();
    card.source = store.snapshot!.sources[0];
    document.body.append(card);
    await card.updateComplete;
    return card;
  }

  it("places safe-copy and mounted indicators beside Browse media", async () => {
    const card = await renderCard();
    const browse = [...card.querySelectorAll("button")].find((button) =>
      button.textContent?.includes("Browse media"),
    )!;
    const row = browse.parentElement!;
    const safeCopies = card.querySelector('[aria-label="View safe-copy details"]')!;
    const mounted = card.querySelector('[role="status"][aria-label="Mounted"]')!;

    expect(row.className).toContain("flex-wrap");
    expect(row.contains(safeCopies)).toBe(true);
    expect(row.contains(mounted)).toBe(true);
    expect(card.textContent).not.toMatch(/Mounted\s*$/);
  });

  it("provides mounted-volume details through a focusable, non-intercepting tooltip", async () => {
    const card = await renderCard();
    const mounted = card.querySelector('[role="status"][aria-label="Mounted"]') as HTMLElement;
    const tooltipId = mounted.getAttribute("aria-describedby")!;
    const tooltip = card.querySelector(`#${tooltipId}`)!;

    expect(mounted.tabIndex).toBe(0);
    expect(tooltip.getAttribute("role")).toBe("tooltip");
    expect(tooltip.textContent).toContain("CAM_A_01");
    expect(tooltip.textContent).toContain("/Volumes/CAM_A_01");
    expect(tooltip.textContent).toContain("free of");
    expect(tooltip.classList.contains("pointer-events-none")).toBe(true);
    mounted.focus();
    expect(document.activeElement).toBe(mounted);
    expect(mounted.getAttribute("aria-describedby")).toBe(tooltipId);
  });

  it("keeps the unmounted state and safe-copy details action when no copies are safe", async () => {
    const st = store.status!.sources.find((item) => item.source_id === "s1")!;
    st.available = false;
    st.safe_copies = 0;
    st.required_copies = 2;
    store.volumes = [];
    const card = await renderCard();

    const mounted = card.querySelector('[role="status"][aria-label="Not mounted"]') as HTMLElement;
    expect(mounted.textContent).toContain("Not mounted");
    const tooltipId = mounted.getAttribute("aria-describedby")!;
    expect(card.querySelector(`#${tooltipId}`)?.textContent).toContain(
      "Camera A · Card 1 is not currently available on this computer.",
    );
    expect(card.textContent?.replace(/\s+/g, " ")).toContain("0/2 safe copies");
    card.querySelector<HTMLButtonElement>('[aria-label="View safe-copy details"]')!.click();
    expect(store.dialogs.at(-1)).toMatchObject({ type: "safe-copy", sourceId: "s1" });
  });
});
