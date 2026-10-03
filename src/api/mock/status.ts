import type { DestinationStatus, FlowStatus, ProjectStatus, Snapshot, SourceStatus } from "../types";

export type Counts = Record<string, [number, number, number, number]>;
const AVG_FILE = 39_000_000;

/** Computes a believable project status from simulated flow counts. */
export function mockStatus(snapshot: Snapshot, projectId: string, counts: Counts, offline: Set<string>): ProjectStatus {
  const project = snapshot.projects.find((p) => p.id === projectId);
  const spaceId = project?.space_id;
  const deviceOf = (id: string) => snapshot.devices.find((d) => d.id === id);
  const flows = snapshot.flows.filter((f) => f.space_id === spaceId);
  const flowStatuses: FlowStatus[] = flows.map((f) => {
    const [transferred, toTransfer, ignored, failed] = counts[f.id] ?? [0, 25, 0, 0];
    const dest = snapshot.destinations.find((d) => d.id === f.destination_id);
    const src = snapshot.sources.find((s) => s.id === f.source_id);
    const unavailable = offline.has(dest?.device_id ?? "") || offline.has(src?.device_id ?? "");
    const state = failed ? "error" : toTransfer ? (unavailable ? "unavailable" : "pending") : transferred ? "done" : "empty";
    return {
      flow_id: f.id,
      state,
      transferred,
      to_transfer: toTransfer,
      ignored,
      failed,
      bytes_to_transfer: toTransfer * AVG_FILE,
      error: failed ? `hash mismatch · ${deviceOf(src?.device_id ?? "")?.name ?? ""}` : null,
    };
  });
  const finals = snapshot.devices.filter((d) => d.role === "final").map((d) => d.id);
  const sources: SourceStatus[] = snapshot.sources
    .filter((s) => s.space_id === spaceId)
    .map((s) => {
      const outgoing = flows.filter((f) => f.source_id === s.id);
      const total = Math.max(0, ...outgoing.map((f) => (counts[f.id] ?? [0, 25, 0, 0]).reduce((a, b) => a + b, 0)));
      const finalFlows = outgoing.filter((f) => finals.includes(snapshot.destinations.find((d) => d.id === f.destination_id)?.device_id ?? ""));
      const safe = finalFlows.filter((f) => (counts[f.id]?.[1] ?? 1) === 0 && (counts[f.id]?.[3] ?? 0) === 0).length;
      const required = project?.final_copies_required ?? 2;
      const missing = finalFlows.filter((f) => (counts[f.id]?.[1] ?? 1) > 0).map((f) => {
        const dest = snapshot.destinations.find((d) => d.id === f.destination_id);
        const dev = deviceOf(dest?.device_id ?? "");
        return offline.has(dev?.id ?? "") ? `${dev?.name} offline` : `Needs ${dev?.name}`;
      });
      const reason = safe >= required ? null : (missing[0] ?? "No final destination");
      return {
        source_id: s.id,
        available: !offline.has(s.device_id),
        root_path: snapshot.mappings.find((m) => m.device_id === s.device_id)?.root_path ?? null,
        file_count: total,
        total_bytes: total * AVG_FILE,
        safe_copies: safe,
        required_copies: required,
        wipe_eligible: reason === null && s.offer_wipe,
        blocking_reason: reason,
      };
    });
  const destinations: DestinationStatus[] = snapshot.destinations
    .filter((d) => d.space_id === spaceId)
    .map((d) => {
      const own = flowStatuses.filter((f) => flows.find((x) => x.id === f.flow_id)?.destination_id === d.id);
      const sum = (key: "transferred" | "to_transfer" | "ignored" | "failed" | "bytes_to_transfer") => own.reduce((a, f) => a + f[key], 0);
      const available = !offline.has(d.device_id);
      return {
        destination_id: d.id,
        available,
        root_path: available ? (snapshot.mappings.find((m) => m.device_id === d.device_id)?.root_path ?? null) : null,
        free_bytes: available ? 1.2e12 : null,
        transferred: sum("transferred"),
        to_transfer: sum("to_transfer"),
        ignored: sum("ignored"),
        failed: sum("failed"),
        bytes_to_transfer: sum("bytes_to_transfer"),
        last_error: own.find((f) => f.error)?.error ?? null,
      };
    });
  return { project_id: projectId, flows: flowStatuses, sources, destinations };
}
