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

  it.each([
    [0, "badge-error"],
    [1, "badge-warning"],
    [2, "badge-success"],
  ])("renders %s copies as a compact colored native badge", async (copies, tone) => {
    store.status!.sources[0].safe_copies = copies as number;
    store.status!.sources[0].required_copies = 2;
    const card = await renderCard();
    const badge = card.querySelector('[aria-label="View safe-copy details"]')!;
    expect(badge.classList.contains(tone as string)).toBe(true);
    expect(badge.classList.contains("badge")).toBe(true);
    expect(badge.classList.contains("btn")).toBe(false);
    expect(badge.querySelector(".badge")).toBeNull();
  });

  it("shows Wipe card only for mounted sources with wiping enabled", async () => {
    const card = await renderCard();
    expect(card.textContent).toContain("Wipe card");
    expect(card.querySelector("[data-source-wipe-footer]")?.textContent).toContain("Wipe card");
    store.status!.sources[0].available = false;
    card.requestUpdate();
    await card.updateComplete;
    expect(card.textContent).not.toContain("Wipe card");
    store.status!.sources[0].blocking_reason = "Wipe safety is checked against every active project";
    expect(card.textContent).not.toContain("Wipe safety");
    expect(card.querySelector("[data-source-wipe-footer]")).toBeNull();
    card.source = { ...card.source, offer_wipe: false };
    await card.updateComplete;
    expect(card.textContent).not.toContain("Wipe safety");
    expect(card.textContent).not.toContain("Wipe card");
    expect(card.textContent).toContain("safe copies");
    expect(card.textContent).toContain("Not mounted");
  });

  it("keeps wipe-blocking explanations in the mount tooltip, not below the indicator row", async () => {
    store.status!.sources[0].available = false;
    store.status!.sources[0].blocking_reason = "Connect the card before wiping";
    const card = await renderCard();

    const mounted = card.querySelector('[role="status"][aria-label="Not mounted"]')!;
    const tooltip = card.querySelector(`#${mounted.getAttribute("aria-describedby")}`)!;
    expect(tooltip.textContent).toContain("Connect the card before wiping");
    expect(card.textContent).not.toContain("Wipe card");
    expect(card.querySelector("[data-source-wipe-footer]")).toBeNull();

    card.source = { ...card.source, offer_wipe: false };
    await card.updateComplete;
    const updatedMounted = card.querySelector('[role="status"][aria-label="Not mounted"]')!;
    const updatedTooltip = card.querySelector(`#${updatedMounted.getAttribute("aria-describedby")}`)!;
    expect(updatedTooltip.textContent).not.toContain("Connect the card before wiping");
  });

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
