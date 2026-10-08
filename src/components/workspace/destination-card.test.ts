import { afterEach, describe, expect, it, vi } from "vitest";
import { demoCounts, demoSnapshot } from "../../api/mock/data";
import { mockStatus } from "../../api/mock/status";
import { store } from "../../state";
import { formatBytes } from "../../utils/format";
import { OmbDestinationCard } from "./destination-card";
import type { OmbCardContextMenu } from "./card-context-menu";
import "../dialogs/omb-dialog-host";

describe("app destination card", () => {
  const previousSnapshot = store.snapshot;
  const previousStatus = store.status;
  const previousDialogs = store.dialogs;
  const previousTransfers = store.transfers;
  const previousBackend = {
    openFlowInApp: store.backend.openFlowInApp,
    confirmAppImport: store.backend.confirmAppImport,
    openWorkspaceFlowInApp: store.backend.openWorkspaceFlowInApp,
    confirmWorkspaceAppImport: store.backend.confirmWorkspaceAppImport,
  };
  let card: OmbDestinationCard | undefined;
  let host: HTMLElement | undefined;

  afterEach(() => {
    card?.remove();
    host?.remove();
    store.snapshot = previousSnapshot;
    store.status = previousStatus;
    store.dialogs = previousDialogs;
    store.transfers = previousTransfers;
    vi.restoreAllMocks();
    store.backend.openFlowInApp = previousBackend.openFlowInApp;
    store.backend.confirmAppImport = previousBackend.confirmAppImport;
    store.backend.openWorkspaceFlowInApp = previousBackend.openWorkspaceFlowInApp;
    store.backend.confirmWorkspaceAppImport = previousBackend.confirmWorkspaceAppImport;
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
    expect(
      [...card.querySelectorAll("button")].some((button) => button.textContent?.trim() === "Check"),
    ).toBe(false);
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

  it.each([
    ["d2", "nas", false],
    ["d1", "ssd", true],
  ] as const)("shows free space for %s only when it is not NAS storage", async (id, kind, showsSpace) => {
    const snapshot = demoSnapshot();
    store.snapshot = snapshot;
    store.status = mockStatus(snapshot, "trip", structuredClone(demoCounts), new Set());
    const destination = snapshot.destinations.find((d) => d.id === id)!;
    const device = snapshot.devices.find((d) => d.id === destination.device_id)!;
    expect(device.kind).toBe(kind);
    const status = store.status.destinations.find((d) => d.destination_id === id)!;
    status.available = true;
    status.free_bytes = 1.2e12;
    card = new OmbDestinationCard();
    card.destination = destination;
    document.body.append(card);
    await card.updateComplete;

    expect(card.textContent).toContain(device.description);
    expect(card.textContent).toContain("Online");
    expect(card.textContent?.includes(`${formatBytes(status.free_bytes)} free`)).toBe(showsSpace);
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
    store.backend.openWorkspaceFlowInApp = vi.fn(async () => ({
      token: "token",
      app_name: "Lightroom",
      files: [{ rel_path: "100MSDCF/IMG_07412.JPG", project_id: null }],
    }));
    store.backend.confirmWorkspaceAppImport = vi.fn(async () => "mark-job");

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
    expect(store.backend.confirmWorkspaceAppImport).toHaveBeenCalledWith(
      { spaceId: "travel", projectId: null },
      "f7",
      "token",
    );
    expect(store.dialogs).toHaveLength(0);
    expect(store.toasts.at(-1)?.message).toContain("queued");
  });

  it("opens the scope chooser for fully transferred destinations without starting jobs", async () => {
    const snapshot = demoSnapshot();
    store.snapshot = snapshot;
    store.status = mockStatus(snapshot, "trip", structuredClone(demoCounts), new Set());
    store.transfers = [];
    store.dialogs = [];
    const check = vi.spyOn(store, "checkDestination").mockResolvedValue(true);
    card = new OmbDestinationCard();
    card.destination = snapshot.destinations.find((d) => d.id === "d1")!;
    document.body.append(card);
    await card.updateComplete;
    const button = [...card.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Check")!;
    expect(button.disabled).toBe(false);
    button.click();
    expect(check).not.toHaveBeenCalled();
    expect(store.dialogs[0]).toEqual({
      type: "destination-check",
      destinationId: "d1",
      context: store.context,
    });
  });

  it("runs all incoming flows in one queue and disables Check while starting", async () => {
    const snapshot = demoSnapshot();
    store.snapshot = snapshot;
    store.status = mockStatus(snapshot, "trip", structuredClone(demoCounts), new Set());
    store.transfers = [];
    let finish!: () => void;
    const run = vi.spyOn(store, "runDestination").mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const flow = vi.spyOn(store, "runFlow").mockResolvedValue();
    card = new OmbDestinationCard();
    card.destination = snapshot.destinations.find((d) => d.id === "d2")!;
    document.body.append(card);
    await card.updateComplete;
    [...card.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Run")!.click();
    await card.updateComplete;
    expect(run).toHaveBeenCalledOnce();
    expect(run).toHaveBeenCalledWith("d2");
    expect(flow).not.toHaveBeenCalled();
    expect(
      [...card.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Check")!.disabled,
    ).toBe(true);
    finish();
  });

  it("allows scanning with offline sources but disables Check for active check jobs", async () => {
    const snapshot = demoSnapshot();
    store.snapshot = snapshot;
    store.status = mockStatus(snapshot, "trip", structuredClone(demoCounts), new Set(["card1", "card2"]));
    store.transfers = [];
    card = new OmbDestinationCard();
    card.destination = snapshot.destinations.find((d) => d.id === "d2")!;
    document.body.append(card);
    await card.updateComplete;
    expect(
      [...card.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Check")!.disabled,
    ).toBe(false);
    store.status = mockStatus(snapshot, "trip", structuredClone(demoCounts), new Set());
    store.transfers = [
      {
        id: "checking",
        flow_id: "check:f4",
        label: "Check",
        kind: "check",
        state: "queued",
        files_done: 0,
        files_total: 1,
        bytes_done: 0,
        bytes_total: 1,
        current_file: null,
        speed_bps: 0,
        bytes_per_sec: null,
        eta_secs: null,
        errors: [],
      },
    ];
    card.requestUpdate();
    await card.updateComplete;
    expect(
      [...card.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Check")!.disabled,
    ).toBe(true);
  });

  it("opens the same chooser from context-menu Check", async () => {
    const snapshot = demoSnapshot();
    store.snapshot = snapshot;
    store.status = mockStatus(snapshot, "trip", structuredClone(demoCounts), new Set());
    store.transfers = [];
    store.dialogs = [];
    const check = vi.spyOn(store, "checkDestination").mockResolvedValue(true);
    card = new OmbDestinationCard();
    card.destination = snapshot.destinations.find((d) => d.id === "d2")!;
    document.body.append(card);
    await card.updateComplete;
    card
      .querySelector("article")!
      .dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 20, clientY: 20 }),
      );
    await card.updateComplete;
    await card.querySelector<OmbCardContextMenu>("omb-card-context-menu")!.updateComplete;
    [...card.querySelectorAll<HTMLButtonElement>("omb-card-context-menu button")]
      .find((b) => b.textContent?.trim() === "Check destination")!
      .click();
    expect(check).not.toHaveBeenCalled();
    expect(store.dialogs[0]?.type).toBe("destination-check");
  });
});
