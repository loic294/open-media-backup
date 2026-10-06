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
import { sourceBackupName } from "../../utils/names";
import { mockSafeCopyDetails } from "./safe-copies";

export type Counts = Record<string, [number, number, number, number]>;
const AVG_FILE = 39_000_000;

/** Computes a believable project status from simulated flow counts. */
export function mockStatus(
  snapshot: Snapshot,
  projectId: string,
  counts: Counts,
  offline: Set<string>,
  safeSkipCounts: Record<string, number> = {},
): ProjectStatus {
  const project = snapshot.projects.find((p) => p.id === projectId);
  if (!project) throw new Error(`Project ${projectId} not found`);
  const status = mockWorkspaceStatus(
    snapshot,
    { spaceId: project.space_id, projectId },
    counts,
    offline,
    safeSkipCounts,
  );
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
  safeSkipCounts: Record<string, number> = {},
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
    if (!src || !dest) throw new Error("Flow source or destination not found");
    const missingSource = !deviceOf(src.device_id);
    const missingDestination = dest.kind !== "app" && !deviceOf(dest.device_id);
    if (missingSource || missingDestination) {
      const task = missingSource ? "source" : "destination";
      return {
        flow_id: f.id,
        state: "unavailable",
        transferred: 0,
        to_transfer: 0,
        ignored: 0,
        failed: 0,
        bytes_to_transfer: 0,
        runnable: false,
        error: `No device selected. Choose a device in ${task} settings.`,
      };
    }
    let configError: string | null = null;
    if (space && src && dest) {
      try {
        const projects = snapshot.projects.filter((p) => p.space_id === spaceId && !p.archived);
        validateProjectRanges(projects, space.allow_project_overlap ?? true);
        validateSourceDestination(src, dest, space, projects);
        if (!project && !snapshot.projects.some((p) => p.space_id === spaceId && !p.archived)) {
          const vars = previewVars(space, null, sourceBackupName(src, deviceOf(src.device_id)));
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
  const sources: SourceStatus[] = snapshot.sources
    .filter((s) => s.space_id === spaceId)
    .map((s) => {
      const sourceDevice = deviceOf(s.device_id);
      if (!sourceDevice)
        return {
          source_id: s.id,
          available: false,
          root_path: null,
          file_count: 0,
          total_bytes: 0,
          safe_copies: 0,
          required_copies: project?.final_copies_required ?? null,
          wipe_eligible: false,
          blocking_reason: "No device selected. Choose a device in source settings.",
        };
      const outgoing = flows.filter((f) => f.source_id === s.id);
      const total = Math.max(
        0,
        ...outgoing.map((f) => (counts[f.id] ?? [0, 25, 0, 0]).reduce((a, b) => a + b, 0)),
      );
      const details = mockSafeCopyDetails(snapshot, context, s.id, counts, offline, safeSkipCounts);
      return {
        source_id: s.id,
        available: !offline.has(s.device_id),
        root_path: snapshot.mappings.find((m) => m.device_id === s.device_id)?.root_path ?? null,
        file_count: total,
        total_bytes: total * AVG_FILE,
        safe_copies: details.safe_copies,
        required_copies: details.required_copies,
        wipe_eligible: details.wipe_eligible,
        blocking_reason: details.blocking_reason,
      };
    });
  const destinations: DestinationStatus[] = snapshot.destinations
    .filter((d) => d.space_id === spaceId)
    .map((d) => {
      const own = flowStatuses.filter((f) => flows.find((x) => x.id === f.flow_id)?.destination_id === d.id);
      const sum = (key: "transferred" | "to_transfer" | "ignored" | "failed" | "bytes_to_transfer") =>
        own.reduce((a, f) => a + f[key], 0);
      const isApp = (d.kind ?? "folder") === "app";
      const assigned = !!deviceOf(d.device_id);
      const available = isApp || (assigned && !offline.has(d.device_id));
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
        last_error:
          !isApp && !assigned
            ? "No device selected. Choose a device in destination settings."
            : (own.find((f) => f.error)?.error ?? null),
      };
    });
  return { context: structuredClone(context), flows: flowStatuses, sources, destinations };
}
