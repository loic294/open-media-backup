import type {
  DestinationStatus,
  FlowStatus,
  ProjectStatus,
  Snapshot,
  SourceStatus,
  WorkspaceContext,
  WorkspaceStatus,
} from "../types";
import { usesProjectVariables, validateProjectRanges, validateSourceDestination } from "../../state/projects";
import { previewVars, templateVars } from "../../utils/template";

export type Counts = Record<string, [number, number, number, number]>;
const AVG_FILE = 39_000_000;

/** Computes a believable project status from simulated flow counts. */
export function mockStatus(
  snapshot: Snapshot,
  projectId: string,
  counts: Counts,
  offline: Set<string>,
): ProjectStatus {
  const project = snapshot.projects.find((p) => p.id === projectId);
  if (!project) throw new Error(`Project ${projectId} not found`);
  const status = mockWorkspaceStatus(snapshot, { spaceId: project.space_id, projectId }, counts, offline);
  return {
    project_id: projectId,
    flows: status.flows,
    sources: status.sources,
    destinations: status.destinations,
  };
}

export function mockWorkspaceStatus(
  snapshot: Snapshot,
  context: WorkspaceContext,
  counts: Counts,
  offline: Set<string>,
): WorkspaceStatus {
  const { spaceId, projectId } = context;
  const space = snapshot.spaces.find((s) => s.id === spaceId);
  const project = projectId ? snapshot.projects.find((p) => p.id === projectId) : undefined;
  if (!space) throw new Error(`Space ${spaceId} not found`);
  if (projectId !== null && !project) throw new Error(`Project ${projectId} not found`);
  if (project && project.space_id !== spaceId) throw new Error("Project must belong to the workspace space");
  const deviceOf = (id: string) => snapshot.devices.find((d) => d.id === id);
  const flows = snapshot.flows.filter((f) => f.space_id === spaceId);
  const flowStatuses: FlowStatus[] = flows.map((f) => {
    const [transferred, toTransfer, ignored, failed] = counts[f.id] ?? [0, 25, 0, 0];
    const dest = snapshot.destinations.find((d) => d.id === f.destination_id);
    const src = snapshot.sources.find((s) => s.id === f.source_id);
    let configError: string | null = null;
    if (space && src && dest) {
      try {
        const projects = snapshot.projects.filter((p) => p.space_id === spaceId);
        validateProjectRanges(projects, space.allow_project_overlap ?? true);
        validateSourceDestination(src, dest, space, projects);
        if (!project) {
          const vars = previewVars(space, null, deviceOf(src.device_id)?.name);
          if (templateVars(src.path_template).some((name) => !(name in vars)))
            throw new Error("Source path requires project values or an unknown variable");
          if ((dest.kind ?? "folder") === "folder") {
            if (usesProjectVariables(dest.path_template, space, projects, dest.use_backup_marker))
              throw new Error(
                "Destination path requires project values; create a project or use a project-independent path",
              );
            if (dest.use_backup_marker) {
              const marker = space.backup_marker_template || "{date}_{project_name}";
              if (templateVars(marker).some((name) => !(name in vars)))
                throw new Error(
                  "Backup folder requires project values; create a project or use a project-independent marker template",
                );
              vars.backup_folder = "Demo backup folder";
            }
            if (templateVars(dest.path_template).some((name) => !(name in vars)))
              throw new Error("Destination path has an unknown variable");
          }
        }
      } catch (error) {
        configError = String(error);
      }
    }
    const unavailable =
      (dest?.kind !== "app" && offline.has(dest?.device_id ?? "")) || offline.has(src?.device_id ?? "");
    const state =
      configError || failed
        ? "error"
        : toTransfer
          ? unavailable
            ? "unavailable"
            : "pending"
          : transferred
            ? "done"
            : "empty";
    return {
      flow_id: f.id,
      state,
      transferred,
      to_transfer: toTransfer,
      ignored,
      failed,
      bytes_to_transfer: toTransfer * AVG_FILE,
      error: configError ?? (failed ? `hash mismatch · ${deviceOf(src?.device_id ?? "")?.name ?? ""}` : null),
      runnable: !configError && !unavailable && toTransfer + failed > 0,
    };
  });
  const finals = snapshot.devices.filter((d) => d.role === "final").map((d) => d.id);
  const temporary = snapshot.devices.filter((d) => d.role === "temporary").map((d) => d.id);
  const sources: SourceStatus[] = snapshot.sources
    .filter((s) => s.space_id === spaceId)
    .map((s) => {
      const sourceDevice = deviceOf(s.device_id);
      const temporaryCopiesPerFinal = Math.max(
        0,
        Math.floor(snapshot.spaces.find((space) => space.id === spaceId)?.temporary_copies_per_final ?? 0),
      );
      const outgoing = flows.filter((f) => f.source_id === s.id);
      const total = Math.max(
        0,
        ...outgoing.map((f) => (counts[f.id] ?? [0, 25, 0, 0]).reduce((a, b) => a + b, 0)),
      );
      const safeCopyFlows = outgoing.filter((f) => {
        const dest = snapshot.destinations.find((d) => d.id === f.destination_id);
        if (!dest?.counts_as_safe_copy) return false;
        const deviceId = dest.device_id;
        return finals.includes(deviceId) || (temporaryCopiesPerFinal > 0 && temporary.includes(deviceId));
      });
      const complete = (f: (typeof outgoing)[number]) =>
        (counts[f.id]?.[1] ?? 1) === 0 && (counts[f.id]?.[3] ?? 0) === 0;
      const finalSafe = safeCopyFlows.filter(
        (f) =>
          finals.includes(snapshot.destinations.find((d) => d.id === f.destination_id)?.device_id ?? "") &&
          complete(f),
      );
      const temporarySafe = safeCopyFlows.filter(
        (f) =>
          temporary.includes(snapshot.destinations.find((d) => d.id === f.destination_id)?.device_id ?? "") &&
          complete(f),
      ).length;
      const safe =
        finalSafe.length +
        (temporaryCopiesPerFinal > 0 ? Math.floor(temporarySafe / temporaryCopiesPerFinal) : 0);
      const required = project?.final_copies_required ?? null;
      const missing = safeCopyFlows
        .filter((f) => (counts[f.id]?.[1] ?? 1) > 0)
        .map((f) => {
          const dest = snapshot.destinations.find((d) => d.id === f.destination_id);
          const dev = deviceOf(dest?.device_id ?? "");
          return offline.has(dev?.id ?? "") ? `${dev?.name} offline` : `Needs ${dev?.name}`;
        });
      const reason =
        required === null
          ? "Create a project to set card-wiping safety requirements"
          : sourceDevice?.role === "final"
            ? "Final devices are never wiped"
            : sourceDevice?.role === "temporary" && finalSafe.length === 0
              ? "Needs a final destination"
              : safe >= required
                ? null
                : (missing[0] ?? "No safe destination");
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
      const sum = (key: "transferred" | "to_transfer" | "ignored" | "failed" | "bytes_to_transfer") =>
        own.reduce((a, f) => a + f[key], 0);
      const isApp = (d.kind ?? "folder") === "app";
      const available = isApp || !offline.has(d.device_id);
      return {
        destination_id: d.id,
        available,
        root_path: isApp
          ? (snapshot.settings.app_destinations?.[d.id] ?? null)
          : available
            ? (snapshot.mappings.find((m) => m.device_id === d.device_id)?.root_path ?? null)
            : null,
        free_bytes: available ? 1.2e12 : null,
        transferred: sum("transferred"),
        to_transfer: sum("to_transfer"),
        ignored: sum("ignored"),
        failed: sum("failed"),
        bytes_to_transfer: sum("bytes_to_transfer"),
        last_error: own.find((f) => f.error)?.error ?? null,
      };
    });
  return { context: structuredClone(context), flows: flowStatuses, sources, destinations };
}
