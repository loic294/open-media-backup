import type { Snapshot, SourceSafeCopyDetails, WorkspaceContext, SafeCopyFile } from "../types";
import type { Counts } from "./status";
import { mockFiles } from "./files";
import { rulesAllowPath } from "../../utils/file-rules";
import { expandTemplate, previewVars, templateVars } from "../../utils/template";
import { sourceBackupName } from "../../utils/names";

/** Demo-only catalog: deterministic per-file evidence, never used by the real backend. */
export function mockSafeCopyDetails(
  snapshot: Snapshot,
  context: WorkspaceContext,
  sourceId: string,
  counts: Counts,
  offline: Set<string>,
  safeSkipCounts: Record<string, number> = {},
): SourceSafeCopyDetails {
  const source = snapshot.sources.find((item) => item.id === sourceId);
  const space = snapshot.spaces.find((item) => item.id === context.spaceId);
  const project = snapshot.projects.find((item) => item.id === context.projectId) ?? null;
  if (!source || !space || source.space_id !== context.spaceId)
    throw new Error("Source must belong to the workspace space");
  if (context.projectId !== null && (!project || project.space_id !== space.id))
    throw new Error("Project must belong to the workspace space");
  const device = snapshot.devices.find((item) => item.id === source.device_id);
  if (!device) throw new Error("Select a device for this source in its settings");
  const grouped = snapshot.sources.filter(
    (item) => item.space_id === space.id && item.device_id === device.id,
  );
  const views = new Map<
    string,
    { rel: string; sourceId: string; index: number; excluded: boolean; vars: Record<string, string> }[]
  >();
  let pathError: string | null = null;
  for (const item of grouped) {
    const vars = previewVars(
      space,
      item.project_scope?.mode === "none" ? null : project,
      sourceBackupName(item, device),
    );
    if (
      templateVars(item.path_template).some((name) => !vars[name]) ||
      expandTemplate(item.path_template, vars).includes("{")
    ) {
      pathError ??= `Source ${item.id}: Source path cannot be resolved`;
      continue;
    }
    const folder = expandTemplate(item.path_template, vars).replace(/^\/+|\/+$/g, "");
    const outgoing = snapshot.flows.filter((flow) => flow.source_id === item.id);
    const total = Math.max(
      outgoing.length > 0 && outgoing.every((flow) => !counts[flow.id]) ? 25 : 0,
      ...outgoing.map((flow) => (counts[flow.id] ?? [0, 0, 0, 0]).reduce((a, b) => a + b, 0)),
    );
    // Use the same deterministic inventory as the demo browser, without its page-size cap.
    const inventory = Array.from(
      { length: Math.ceil(total / 1000) },
      (_, page) => mockFiles(total, "transferred", page * 1000, 1000).items,
    ).flat();
    inventory.forEach((file, index) => {
      const path = [folder, file.rel_path].filter(Boolean).join("/");
      const entries = views.get(path) ?? [];
      entries.push({
        rel: file.rel_path,
        sourceId: item.id,
        index,
        excluded: !rulesAllowPath(item.safe_copy_rules ?? [], file.rel_path, vars),
        vars,
      });
      views.set(path, entries);
    });
  }
  const ratio = space.temporary_copies_per_final ?? 0;
  const targets = new Map<string, { name: string; role: "final" | "temporary"; flows: Snapshot["flows"] }>();
  for (const flow of snapshot.flows.filter((item) =>
    grouped.some((source) => source.id === item.source_id),
  )) {
    const dest = snapshot.destinations.find((item) => item.id === flow.destination_id);
    const targetDevice = snapshot.devices.find((item) => item.id === dest?.device_id);
    if (
      !dest?.counts_as_safe_copy ||
      (dest.kind !== "app" &&
        (!targetDevice ||
          targetDevice.id === device.id ||
          (targetDevice.role !== "final" && !(targetDevice.role === "temporary" && ratio > 0))))
    )
      continue;
    const key = dest.kind === "app" ? dest.id : dest.device_id;
    const target = targets.get(key) ?? {
      name: dest.task_name || (dest.kind === "app" ? dest.app_name || "Application" : targetDevice!.name),
      role: dest.kind === "app" || targetDevice?.role === "final" ? "final" : "temporary",
      flows: [],
    };
    target.flows.push(flow);
    targets.set(key, target);
  }
  const effective = (keys: string[]) =>
    keys.filter((key) => targets.get(key)!.role === "final").length +
    (ratio > 0 ? Math.floor(keys.filter((key) => targets.get(key)!.role === "temporary").length / ratio) : 0);
  const evidence = [...views]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([path, entries]) => {
      const requiredViews = entries.filter((view) => !view.excluded);
      const verified: string[] = [];
      const acknowledged: string[] = [];
      const reasons: string[] = [];
      for (const [key, target] of targets) {
        const matches = requiredViews.map((view) => {
          const flows = target.flows
            .filter((flow) => flow.source_id === view.sourceId)
            .filter((flow) =>
              rulesAllowPath(
                snapshot.destinations.find((dest) => dest.id === flow.destination_id)!.rules,
                view.rel,
                view.vars,
              ),
            );
          const checked = flows.some((flow) => view.index < (counts[flow.id]?.[0] ?? 0));
          const skipped = flows.some(
            (flow) =>
              space.skip_counts_as_safe_copy &&
              view.index >= (counts[flow.id]?.[0] ?? 0) &&
              view.index < (counts[flow.id]?.[0] ?? 0) + (safeSkipCounts[flow.id] ?? 0),
          );
          return { allowed: flows.length > 0, checked, skipped };
        });
        if (!requiredViews.length) continue;
        if (matches.every((match) => match.checked)) verified.push(key);
        else if (matches.every((match) => match.checked || match.skipped)) acknowledged.push(key);
        else if (matches.some((match) => !match.allowed))
          reasons.push(
            `${target.name}: destination rules exclude this file or no source flow provides coverage; edit destination rules or connect this source`,
          );
        else
          reasons.push(
            offline.has(key)
              ? `${target.name} is offline; reconnect it to transfer or check the missing copy`
              : `${target.name}: not transferred or verified yet; run or check the destination`,
          );
      }
      const copies = effective([...verified, ...acknowledged]);
      const excluded = requiredViews.length === 0;
      const safe =
        project !== null &&
        copies >= project.final_copies_required &&
        (device.role !== "temporary" ||
          [...verified, ...acknowledged].some((key) => targets.get(key)!.role === "final"));
      if (excluded)
        reasons.push("Excluded from safe-copy requirements by source rules; transfers are unchanged");
      else if (!project) reasons.push("Choose a project to inspect its required copy threshold");
      else if (!safe)
        reasons.push(
          `Requires ${project.final_copies_required} effective copies; currently ${copies}. Add safe destinations or finish pending transfers`,
        );
      const file: SafeCopyFile = {
        path,
        state: excluded ? "excluded" : safe ? "safe" : "unsafe",
        safe_copies: copies,
        verified_destinations: verified.map((key) => targets.get(key)!.name),
        acknowledged_destinations: acknowledged.map((key) => targets.get(key)!.name),
        reasons,
      };
      return { file, keys: [...verified, ...acknowledged] };
    });
  const required = evidence.filter((item) => item.file.state !== "excluded");
  const complete = [...targets.keys()].filter(
    (key) => required.length > 0 && required.every((item) => item.keys.includes(key)),
  );
  const safeCopies = effective(complete);
  const hasSourceFiles = [...views.values()].some((entries) =>
    entries.some((view) => view.sourceId === sourceId),
  );
  const hasApplicableProject = snapshot.projects.some(
    (item) =>
      item.space_id === space.id &&
      !item.archived &&
      source.project_scope?.mode !== "none" &&
      (source.project_scope?.mode !== "selected" || source.project_scope.project_ids.includes(item.id)),
  );
  const reason =
    pathError ??
    (offline.has(device.id)
      ? `${device.name} not mounted`
      : !hasSourceFiles
        ? "No files"
        : device.role === "final"
          ? "Final devices are never wiped"
          : !project
            ? hasApplicableProject
              ? "Wipe safety is checked against every active project"
              : "Create a project to set card-wiping safety requirements"
            : required.length === 0
              ? null
              : device.role === "temporary" && !complete.some((key) => targets.get(key)!.role === "final")
                ? "Needs a final destination"
                : safeCopies < project.final_copies_required
                  ? "Required device-wide copy threshold not met; inspect missing file coverage"
                  : null);
  return {
    source_id: source.id,
    device_id: device.id,
    device_name: device.name,
    required_copies: project?.final_copies_required ?? null,
    safe_copies: safeCopies,
    wipe_eligible: reason === null && source.offer_wipe,
    blocking_reason: reason,
    editable: snapshot.mappings.some(
      (mapping) =>
        mapping.device_id === device.id &&
        mapping.computer_id === snapshot.computer.id &&
        !!mapping.root_path.trim(),
    ),
    files: evidence.map((item) => item.file),
  };
}
