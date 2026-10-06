import type { Snapshot } from "../types";
import type { Counts } from "./status";

/** Retires old source contents and copy claims on the externally formatted device. */
export function manuallyWipedCounts(snapshot: Snapshot, counts: Counts, deviceId: string): Counts {
  const next = structuredClone(counts);
  const sources = new Set(
    snapshot.sources.filter((source) => source.device_id === deviceId).map((source) => source.id),
  );
  for (const flow of snapshot.flows) {
    if (sources.has(flow.source_id)) {
      next[flow.id] = [0, 0, 0, 0];
    } else {
      const destination = snapshot.destinations.find((item) => item.id === flow.destination_id);
      if (destination?.kind !== "app" && destination?.device_id === deviceId) {
        const [transferred, pending, ignored, failed] = next[flow.id] ?? [0, 25, 0, 0];
        next[flow.id] = [0, pending + transferred, ignored, failed];
      }
    }
  }
  return next;
}
