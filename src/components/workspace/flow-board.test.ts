import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import type { Status } from "../../api/types";
import { store } from "../../state";
import { installDropdownDismiss } from "../ui/dropdown";
import { filterDestinations, filterSources } from "./device-filter";
import { OmbFlowBoard } from "./flow-board";

describe("flow board mounted device ordering", () => {
  const previousSnapshot = store.snapshot;
  const previousVolumes = store.volumes;
  let board: OmbFlowBoard | undefined;

  afterEach(() => {
    board?.remove();
    board = undefined;
    store.snapshot = previousSnapshot;
    store.volumes = previousVolumes;
  });

  describe("independent source and destination device filters", () => {
    const previousSnapshot = store.snapshot;
    const previousStatus = store.status;
    let board: OmbFlowBoard;

    const cardIds = (tag: "source" | "destination") =>
      [...board.querySelectorAll(`omb-${tag}-card`)].map((card) =>
        tag === "source"
          ? (card as HTMLElement & { source: { id: string } }).source.id
          : (card as HTMLElement & { destination: { id: string } }).destination.id,
      );
    const menus = () => [...board.querySelectorAll<HTMLDetailsElement>("[data-device-filter]")];
    const choose = async (label: string, column = 0) => {
      const menu = menus()[column];
      menu.open = true;
      [...menu.querySelectorAll("button")].find((button) => button.textContent?.trim() === label)!.click();
      await board.updateComplete;
      expect(menu.open).toBe(false);
    };
    const mount = async () => {
      store.snapshot = demoSnapshot();
      store.status = {
        context: { spaceId: "travel", projectId: null },
        flows: [],
        sources: store.snapshot.sources.map((source) => ({
          source_id: source.id,
          available: source.id === "s1",
          root_path: null,
          file_count: 0,
          total_bytes: 0,
          safe_copies: 0,
          required_copies: null,
          wipe_eligible: false,
          blocking_reason: null,
        })),
        destinations: store.snapshot.destinations.map((destination) => ({
          destination_id: destination.id,
          available: destination.id === "d1",
          root_path: null,
          free_bytes: null,
          transferred: 0,
          to_transfer: 0,
          ignored: 0,
          failed: 0,
          bytes_to_transfer: 0,
          last_error: null,
        })),
      } satisfies Status;
      board = new OmbFlowBoard();
      document.body.append(board);
      await board.updateComplete;
    };

    afterEach(() => {
      board?.remove();
      store.snapshot = previousSnapshot;
      store.status = previousStatus;
      vi.restoreAllMocks();
    });

    it("shows all cards by default and places a filter immediately before each add action", async () => {
      await mount();
      expect(cardIds("source")).toHaveLength(3);
      expect(cardIds("destination")).toHaveLength(4);
      for (const menu of menus()) {
        expect(menu.nextElementSibling?.textContent).toMatch(/Add (source|destination)/);
        expect(menu.querySelector('[aria-pressed="true"]')?.textContent?.trim()).toBe("All devices");
      }
    });

    it("filters each column by the same availability used by cards and keeps app destinations", async () => {
      await mount();
      await choose("Mounted devices", 0);
      expect(cardIds("source")).toEqual(["s1"]);
      expect(cardIds("destination")).toHaveLength(4);
      await choose("All devices", 0);
      await choose("Mounted devices", 1);
      expect(cardIds("source")).toHaveLength(3);
      expect(cardIds("destination")).toEqual(["d1", "d4"]);
      expect(menus()[1].querySelector("summary")?.textContent).toContain("Mounted devices");
      expect(menus()[0].querySelector("summary")?.textContent).not.toContain("Mounted devices");
      store.status!.sources[0].available = false;
      store.dispatchEvent(new Event("change"));
      await board.updateComplete;
      await choose("Mounted devices", 0);
      expect(cardIds("source")).toEqual([]);
      expect(board.textContent).toContain("No sources match this filter.");
    });

    it("keeps source and destination device selections independent", async () => {
      await mount();
      store.snapshot!.sources.push({ ...store.snapshot!.sources[0], id: "ssd-source", device_id: "ssd" });
      await choose("Travel SSD", 0);
      expect(cardIds("source")).toEqual(["ssd-source"]);
      expect(cardIds("destination")).toHaveLength(4);
      const destinationId = store.snapshot!.destinations.find((d) => d.device_id && d.kind !== "app")!;
      const deviceName = store.snapshot!.devices.find((d) => d.id === destinationId.device_id)!.name;
      await choose(deviceName, 1);
      expect(cardIds("destination")).toContain(destinationId.id);
      expect(
        cardIds("destination").every(
          (id) =>
            id === "d4" ||
            store.snapshot!.destinations.find((d) => d.id === id)!.device_id === destinationId.device_id,
        ),
      ).toBe(true);
      expect(cardIds("source")).toEqual(["ssd-source"]);
      await choose("All devices", 0);
      expect(cardIds("source")).toHaveLength(4);
      expect(cardIds("destination").length).toBeLessThan(4);
    });

    it("resets safely when the selected device is removed without restoring it if it returns", async () => {
      await mount();
      await choose("Camera A · Card 1");
      store.snapshot!.devices = store.snapshot!.devices.filter((device) => device.id !== "card1");
      store.dispatchEvent(new Event("change"));
      await board.updateComplete;
      expect(cardIds("source")).toHaveLength(3);
      expect(menus()[0].querySelector('[aria-pressed="true"]')?.textContent?.trim()).toBe("All devices");
      store.snapshot!.devices = demoSnapshot().devices;
      store.dispatchEvent(new Event("change"));
      await board.updateComplete;
      expect(cardIds("source")).toHaveLength(3);
    });

    it("shows empty filter results without substituting the add-destination empty state", async () => {
      await mount();
      store.snapshot!.destinations = store.snapshot!.destinations.filter(
        (destination) => destination.kind !== "app",
      );
      await choose("Camera A · Card 1", 1);
      expect(cardIds("destination")).toEqual([]);
      expect(board.textContent).toContain("No destinations match this filter.");
      expect(board.textContent).not.toContain("Add an SSD, NAS or folder");
    });

    it("keeps add and card actions intact, stops dropdown clicks and restores trigger focus", async () => {
      await mount();
      const open = vi.spyOn(store, "open").mockImplementation(() => {});
      const select = vi.spyOn(store, "select").mockImplementation(() => {});
      const click = vi.fn();
      board.addEventListener("click", click);
      await choose("Mounted devices");
      expect(click).not.toHaveBeenCalled();
      expect(select).not.toHaveBeenCalled();
      expect(document.activeElement).toBe(menus()[0].querySelector("summary"));
      for (const menu of menus()) (menu.nextElementSibling as HTMLButtonElement).click();
      expect(open).toHaveBeenCalledWith({ type: "source-settings", sourceId: null });
      expect(open).toHaveBeenCalledWith({ type: "destination-settings", destinationId: null });
      board.querySelector<HTMLElement>("[data-source-id]")!.click();
      expect(select).toHaveBeenCalledWith("s1");
    });

    it("closes on Escape with focus on its native keyboard trigger and on outside pointerdown", async () => {
      await mount();
      installDropdownDismiss();
      const menu = menus()[0];
      menu.open = true;
      const button = menu.querySelector<HTMLButtonElement>("button")!;
      button.focus();
      button.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      expect(menu.open).toBe(false);
      expect(document.activeElement).toBe(menu.querySelector("summary"));
      menu.open = true;
      document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
      expect(menu.open).toBe(false);
    });

    it("requests connection layout measurement when filtering changes the visible cards", async () => {
      await mount();
      const canvas = board.querySelector<OmbFlowBoard>("omb-flow-canvas")!;
      const update = vi.spyOn(canvas, "requestUpdate");
      await choose("Camera A · Card 1");
      expect(update).toHaveBeenCalled();
    });

    it("does not treat unknown status or an unassigned device as mounted, and does not mutate inputs", () => {
      const snapshot = demoSnapshot();
      expect(filterSources(snapshot.sources, null, { kind: "all" })).toBe(snapshot.sources);
      expect(filterDestinations(snapshot.destinations, null, { kind: "all" })).toBe(snapshot.destinations);
      expect(filterSources(snapshot.sources, null, { kind: "mounted" })).toEqual([]);
      expect(
        filterDestinations(snapshot.destinations, null, { kind: "mounted" }).map(
          (destination) => destination.id,
        ),
      ).toEqual(["d4"]);
      expect(snapshot.sources).toHaveLength(3);
      expect(snapshot.destinations).toHaveLength(4);
    });

    it("excludes unassigned cards even if their previous status was available", async () => {
      await mount();
      store.snapshot!.sources[0].device_id = "";
      store.snapshot!.destinations[0].device_id = "";
      await choose("Mounted devices", 0);
      expect(cardIds("source")).toEqual([]);
      await choose("Mounted devices", 1);
      expect(cardIds("destination")).toEqual(["d4"]);
      await choose("All devices", 0);
      await choose("All devices", 1);
      expect(cardIds("source")).toHaveLength(3);
      expect(cardIds("destination")).toHaveLength(4);
    });
  });

  it("shows mounted source and destination cards first while keeping app cards in place", async () => {
    const snapshot = demoSnapshot();
    snapshot.destinations.find((destination) => destination.id === "d4")!.position = 1;
    store.snapshot = snapshot;
    store.volumes = [
      {
        mount_path: "/Volumes/CAM_A_02",
        name: "CAM_A_02",
        volume_uuid: null,
        hw_serial: null,
        total_bytes: null,
        free_bytes: null,
        removable: true,
        device_id: "card2",
        matched_by: "mapping",
      },
      {
        mount_path: "/Volumes/photo",
        name: "photo",
        volume_uuid: null,
        hw_serial: null,
        total_bytes: null,
        free_bytes: null,
        removable: false,
        device_id: "nas",
        matched_by: "mapping",
      },
    ];
    board = new OmbFlowBoard();
    document.body.append(board);
    await board.updateComplete;

    expect(
      [...board.querySelectorAll("omb-source-card")].map(
        (card) => (card as HTMLElement & { source: { id: string } }).source.id,
      ),
    ).toEqual(["s2", "s1", "s3"]);
    expect(
      [...board.querySelectorAll("omb-destination-card")].map(
        (card) => (card as HTMLElement & { destination: { id: string } }).destination.id,
      ),
    ).toEqual(["d2", "d1", "d4", "d3"]);
  });
});
