import { afterEach, describe, expect, it, vi } from "vitest";
import { demoCounts, demoSnapshot } from "../../api/mock/data";
import { mockStatus } from "../../api/mock/status";
import { store } from "../../state";
import { OmbDestinationCard } from "./destination-card";
import "../dialogs/omb-dialog-host";

describe("app destination card", () => {
  const previousSnapshot = store.snapshot;
  const previousStatus = store.status;
  const previousDialogs = store.dialogs;
  const previousBackend = {
    openFlowInApp: store.backend.openFlowInApp,
    confirmAppImport: store.backend.confirmAppImport,
  };
  let card: OmbDestinationCard | undefined;
  let host: HTMLElement | undefined;

  afterEach(() => {
    card?.remove();
    host?.remove();
    store.snapshot = previousSnapshot;
    store.status = previousStatus;
    store.dialogs = previousDialogs;
    store.backend.openFlowInApp = previousBackend.openFlowInApp;
    store.backend.confirmAppImport = previousBackend.confirmAppImport;
  });

  it("renders manual app import details", async () => {
    const snapshot = demoSnapshot();
    snapshot.destinations.find((destination) => destination.id === "d4")!.counts_as_safe_copy = true;
    store.snapshot = snapshot;
    store.status = mockStatus(snapshot, "trip", structuredClone(demoCounts), new Set());
    card = new OmbDestinationCard();
    card.destination = snapshot.destinations.find((d) => d.id === "d4")!;
    document.body.append(card);
    await card.updateComplete;

    expect(card.textContent).toContain("Lightroom · App · manual import");
    expect(card.textContent).toContain("Manual");
    expect(card.querySelector('omb-icon[title="Confirmed imports count as a safe copy"]')).not.toBeNull();
    expect(card.textContent).toContain("to import");
    expect(card.querySelector("button.btn-primary")?.textContent).toContain("Open in Lightroom");
  });

  it("estimates pending transfer time from the learned destination speed", async () => {
    const snapshot = demoSnapshot();
    const counts = structuredClone(demoCounts);
    store.snapshot = snapshot;
    store.status = mockStatus(snapshot, "trip", counts, new Set());
    const status = store.status.destinations.find((d) => d.destination_id === "d2")!;
    snapshot.settings.transfer_speeds = {
      nas: status.bytes_to_transfer / 360,
      _global: status.bytes_to_transfer / 120,
    };
    card = new OmbDestinationCard();
    card.destination = snapshot.destinations.find((d) => d.id === "d2")!;
    document.body.append(card);
    await card.updateComplete;

    expect(card.textContent).toContain("to transfer");
    expect(card.textContent).toContain("~6 min");
    expect(card.textContent).not.toContain("~2 min");
  });

  it("asks to choose the local app when none is configured on this computer", async () => {
    const snapshot = demoSnapshot();
    delete snapshot.settings.app_destinations?.d4;
    store.snapshot = snapshot;
    store.status = mockStatus(snapshot, "trip", structuredClone(demoCounts), new Set());
    store.dialogs = [];
    card = new OmbDestinationCard();
    card.destination = snapshot.destinations.find((d) => d.id === "d4")!;
    document.body.append(card);
    await card.updateComplete;

    expect(card.textContent).toContain("Choose the app for this destination on this computer.");
    const button = [...card.querySelectorAll<HTMLButtonElement>("button")].find((b) =>
      b.textContent?.includes("Choose app"),
    )!;
    expect(button.textContent).toContain("Choose app…");
    button.click();

    expect(store.dialogs[0]).toEqual({ type: "destination-settings", destinationId: "d4" });
  });

  it("opens a confirm dialog and marks imported files", async () => {
    const snapshot = demoSnapshot();
    store.snapshot = snapshot;
    store.status = mockStatus(snapshot, "trip", structuredClone(demoCounts), new Set());
    store.dialogs = [];
    store.backend.openFlowInApp = vi.fn(async () => ({
      token: "token",
      app_name: "Lightroom",
      files: [{ rel_path: "100MSDCF/IMG_07412.JPG", project_id: null }],
    }));
    store.backend.confirmAppImport = vi.fn(async () => 1);

    card = new OmbDestinationCard();
    card.destination = snapshot.destinations.find((d) => d.id === "d4")!;
    host = document.createElement("omb-dialog-host");
    document.body.append(card, host);
    await card.updateComplete;
    card.querySelector<HTMLButtonElement>("button.btn-primary")!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await (host as never as { updateComplete: Promise<unknown> }).updateComplete;

    expect(store.dialogs[0]?.type).toBe("confirm");
    expect(document.body.textContent).toContain("Did Lightroom finish importing?");
    [...document.body.querySelectorAll("button")]
      .find((button) => button.textContent?.includes("Mark as transferred"))!
      .click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(store.backend.confirmAppImport).toHaveBeenCalledWith("trip", "f7", "token");
  });
});
