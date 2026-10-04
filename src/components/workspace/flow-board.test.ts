import { afterEach, describe, expect, it } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
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

    expect([...board.querySelectorAll("omb-source-card")].map((card) => (card as HTMLElement & { source: { id: string } }).source.id))
      .toEqual(["s2", "s1", "s3"]);
    expect(
      [...board.querySelectorAll("omb-destination-card")].map(
        (card) => (card as HTMLElement & { destination: { id: string } }).destination.id,
      ),
    ).toEqual(["d2", "d1", "d4", "d3"]);
  });
});
