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
import { expandTemplate, previewVars, templateVars } from "../../utils/template";
import { sourceBackupName } from "../../utils/names";

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
  const activeProjects = snapshot.projects.filter((p) => p.space_id === spaceId && !p.archived);
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
  const finals = snapshot.devices.filter((d) => d.role === "final").map((d) => d.id);
  const temporary = snapshot.devices.filter((d) => d.role === "temporary").map((d) => d.id);
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
      const temporaryCopiesPerFinal = Math.max(
        0,
        Math.floor(snapshot.spaces.find((space) => space.id === spaceId)?.temporary_copies_per_final ?? 0),
      );
      const outgoing = flows.filter((f) => f.source_id === s.id);
      const total = Math.max(
        0,
        ...outgoing.map((f) => (counts[f.id] ?? [0, 25, 0, 0]).reduce((a, b) => a + b, 0)),
      );
      const groupedSources = snapshot.sources.filter(
        (source) => source.space_id === spaceId && source.device_id === s.device_id,
      );
      const groupedFlows = flows.filter((f) => groupedSources.some((source) => source.id === f.source_id));
      const safeCopyFlows = groupedFlows.filter((f) => {
        const dest = snapshot.destinations.find((d) => d.id === f.destination_id);
        if (!dest?.counts_as_safe_copy) return false;
        if ((dest.kind ?? "folder") === "app") return true;
        const deviceId = dest.device_id;
        if (deviceId === s.device_id) return false;
        return finals.includes(deviceId) || (temporaryCopiesPerFinal > 0 && temporary.includes(deviceId));
      });
      const complete = (f: (typeof outgoing)[number]) =>
        (counts[f.id]?.[1] ?? 1) <= (space.skip_counts_as_safe_copy ? (safeSkipCounts[f.id] ?? 0) : 0) &&
        (counts[f.id]?.[3] ?? 0) === 0 &&
        !flowStatuses.find((status) => status.flow_id === f.id)?.error;
      const targets = new Map<string, { role: "final" | "temporary"; flows: typeof outgoing }>();
      for (const flow of safeCopyFlows) {
        const destination = snapshot.destinations.find((d) => d.id === flow.destination_id)!;
        const isApp = destination.kind === "app";
        const key = isApp ? `app:${destination.id}` : `device:${destination.device_id}`;
        const target = targets.get(key) ?? {
          role: isApp || finals.includes(destination.device_id) ? "final" : "temporary",
          flows: [],
        };
        target.flows.push(flow);
        targets.set(key, target);
      }
      const completeTargets = [...targets.values()].filter((target) => {
        const hasRequiredFiles = target.flows.some(
          (f) =>
            (counts[f.id]?.[0] ?? 0) > 0 ||
            (space.skip_counts_as_safe_copy && (safeSkipCounts[f.id] ?? 0) > 0),
        );
        return (
          hasRequiredFiles &&
          groupedSources.every((source) => {
            const own = groupedFlows.filter((f) => f.source_id === source.id);
            const empty = own.every((f) => (counts[f.id] ?? [0, 25, 0, 0]).every((n) => n === 0));
            const coverage = target.flows.filter((f) => f.source_id === source.id);
            return empty || (coverage.length > 0 && coverage.every(complete));
          })
        );
      });
      const finalSafe = completeTargets.filter((target) => target.role === "final").length;
      const temporarySafe = completeTargets.filter((target) => target.role === "temporary").length;
      const safe =
        finalSafe + (temporaryCopiesPerFinal > 0 ? Math.floor(temporarySafe / temporaryCopiesPerFinal) : 0);
      const required = project?.final_copies_required ?? null;
      const scope = s.project_scope ?? { mode: "all" };
      const hasApplicableProject = activeProjects.some(
        (item) => scope.mode !== "none" && (scope.mode !== "selected" || scope.project_ids.includes(item.id)),
      );
      const missing = safeCopyFlows
        .filter((f) => !complete(f))
        .map((f) => {
          const dest = snapshot.destinations.find((d) => d.id === f.destination_id);
          if (dest?.kind === "app") {
            return `Needs ${dest.task_name?.trim() || dest.app_name || "Application"}`;
          }
          const dev = deviceOf(dest?.device_id ?? "");
          return offline.has(dev?.id ?? "") ? `${dev?.name} offline` : `Needs ${dev?.name}`;
        });
      const unresolvedSource = groupedSources.find((source) => {
        const vars = previewVars(space, project ?? null, sourceBackupName(source, sourceDevice));
        return (
          templateVars(source.path_template).some((name) => !vars[name]) ||
          expandTemplate(source.path_template, vars).includes("{")
        );
      });
      const reason = unresolvedSource
        ? `Source ${unresolvedSource.id}: Source path cannot be resolved`
        : offline.has(s.device_id)
          ? `${sourceDevice.name} not mounted`
          : required === null
            ? hasApplicableProject
              ? "Wipe safety is checked against every active project"
              : "Create a project to set card-wiping safety requirements"
            : total === 0
              ? "No files"
              : sourceDevice?.role === "final"
                ? "Final devices are never wiped"
                : sourceDevice?.role === "temporary" && finalSafe === 0
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
