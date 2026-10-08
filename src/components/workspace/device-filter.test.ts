import { describe, expect, it } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import type { Status } from "../../api/types";
import { filterDestinations, filterSources } from "./device-filter";

function fixtures() {
  const snapshot = demoSnapshot();
  const status: Status = {
    context: { spaceId: "travel", projectId: null },
    flows: [],
    sources: snapshot.sources.map((source, index) => ({
      source_id: source.id,
      available: index === 0,
      root_path: null,
      file_count: 0,
      total_bytes: 0,
      safe_copies: 0,
      required_copies: null,
      wipe_eligible: false,
      blocking_reason: null,
    })),
    destinations: snapshot.destinations.map((destination, index) => ({
      destination_id: destination.id,
      available: index === 0,
      root_path: null,
      free_bytes: null,
      transferred: 0,
      to_transfer: 0,
      ignored: 0,
      failed: 0,
      bytes_to_transfer: 0,
      last_error: null,
    })),
  };
  return { snapshot, status };
}

describe("device filter visibility", () => {
  it.each(["unavailable", "missing", "unknown"] as const)(
    "includes manual apps under Mounted devices with %s status without changing state",
    (availability) => {
      const { snapshot, status } = fixtures();
      const app = snapshot.destinations.find((destination) => destination.kind === "app")!;
      if (availability === "missing") {
        status.destinations = status.destinations.filter(
          (destination) => destination.destination_id !== app.id,
        );
      }
      const inputStatus = availability === "unknown" ? null : status;
      const before = structuredClone({ snapshot, status });

      const visible = filterDestinations(snapshot.destinations, inputStatus, { kind: "mounted" });

      expect(visible.map((destination) => destination.id)).toEqual(
        availability === "unknown" ? ["d4"] : ["d1", "d4"],
      );
      expect(visible).toContain(app);
      expect({ snapshot, status }).toEqual(before);
      expect(app.device_id).toBe("");
    },
  );

  it("preserves folder availability and device assignment requirements, including legacy folders", () => {
    const { snapshot, status } = fixtures();
    const folder = snapshot.destinations[0];
    delete folder.kind;
    expect(filterDestinations(snapshot.destinations, status, { kind: "mounted" })).toEqual([
      folder,
      snapshot.destinations[3],
    ]);
    folder.device_id = "";
    expect(filterDestinations(snapshot.destinations, status, { kind: "mounted" })).toEqual([
      snapshot.destinations[3],
    ]);
  });

  it("preserves All devices, individual device filtering, and source filtering", () => {
    const { snapshot, status } = fixtures();
    expect(filterDestinations(snapshot.destinations, status, { kind: "all" })).toBe(snapshot.destinations);
    expect(filterSources(snapshot.sources, status, { kind: "all" })).toBe(snapshot.sources);
    expect(filterDestinations(snapshot.destinations, status, { kind: "device", deviceId: "nas" })).toEqual([
      snapshot.destinations[1],
      snapshot.destinations[3],
    ]);
    expect(filterSources(snapshot.sources, status, { kind: "mounted" })).toEqual([snapshot.sources[0]]);
    expect(filterSources(snapshot.sources, null, { kind: "mounted" })).toEqual([]);
    expect(filterSources(snapshot.sources, status, { kind: "device", deviceId: "card2" })).toEqual([
      snapshot.sources[1],
    ]);
    snapshot.sources[0].device_id = "";
    expect(filterSources(snapshot.sources, status, { kind: "mounted" })).toEqual([]);
  });
});
