import type { Backend } from "../backend";
import type {
  ConflictDecision,
  DestinationCheckItem,
  DestinationCheckResults,
  EntityKind,
  MediaMetadata,
  Snapshot,
  TransferJob,
  UpdateInfo,
  Volume,
  WorkspaceContext,
} from "../types";
import { demoCounts, demoOffline, demoSnapshot } from "./data";
import { Emitter } from "./emitter";
import { mockFiles } from "./files";
import { mockStatus, mockWorkspaceStatus, type Counts } from "./status";
import { demoSync } from "./sync";
import { validateProjectConfiguration } from "../../state/projects";
import { flowLabel } from "../../state/selectors";
import { sanitizeBackupName, sourceBackupName } from "../../utils/names";
import { expandTemplate, previewVars, templateVars } from "../../utils/template";

const COLLECTION: Record<EntityKind, keyof Snapshot> = {
  space: "spaces",
  project: "projects",
  device: "devices",
  device_mapping: "mappings",
  computer: "computers",
  source: "sources",
  destination: "destinations",
  flow: "flows",
};
const AVG_FILE = 39_000_000;

function checkSummary(items: DestinationCheckItem[]): DestinationCheckResults {
  return {
    matched: items.filter((item) => item.outcome === "matched").length,
    missing: items.filter((item) => item.outcome === "missing").length,
    conflicts: items.filter((item) => item.outcome === "conflict").length,
    errors: items.filter((item) => item.outcome === "error").length,
    items: items.filter((item) => item.outcome !== "matched"),
  };
}

/** In-memory backend used in a plain browser (and in UI tests). Simulates transfers. */
export function createMockBackend(
  options: {
    tickMs?: number;
    seedRunningTransfer?: boolean;
    demoUpdate?: boolean;
    /** Simulated existing same-name files with different hashes, for demo/UI tests. */
    conflicts?: Record<string, number>;
  } = {},
): Backend {
  const snapshot = demoSnapshot();
  const counts: Counts = structuredClone(demoCounts);
  const offline = new Set(demoOffline);
  const events = new Emitter();
  const jobs: TransferJob[] = [];
  const paused = new Set<string>();
  const queueByJob = new Map<string, string>();
  const queueDecisions = new Map<string, ConflictDecision>();
  const conflictsRemaining = new Map<string, number>();
  const checkItemsByJob = new Map<string, DestinationCheckItem[]>();
  const skippedJobs = new Set<string>();
  let timer: ReturnType<typeof setInterval> | undefined;
  let pendingUpdate: UpdateInfo | null = options.demoUpdate
    ? {
        version: "0.2.0",
        current_version: "0.1.0",
        notes:
          "- Faster BLAKE3 verification\n- Transfer speed estimates for pending jobs\n- Improved project variables and file rules",
        date: "2026-10-03",
      }
    : null;

  const list = (kind: EntityKind) => snapshot[COLLECTION[kind]] as unknown as { id: string }[];
  const changed = () => {
    events.emit("snapshot-changed");
    events.emit("status-changed");
  };

  const decide = (job: TransferJob, decision: ConflictDecision) => {
    if (decision === "skip") skippedJobs.add(job.id);
    const c = counts[job.flow_id];
    if (decision !== "skip" && c) {
      c[0]++;
      c[1] = Math.max(0, c[1] - 1);
    }
    job.files_done++;
    job.bytes_done = job.files_done * AVG_FILE;
    conflictsRemaining.set(job.id, Math.max(0, (conflictsRemaining.get(job.id) ?? 0) - 1));
    job.pending_conflict = null;
    job.state = paused.has(job.id) ? "paused" : "running";
  };

  const tick = () => {
    for (const job of jobs.filter((j) => j.state === "running" || j.state === "queued")) {
      if (paused.has(job.id)) continue;
      job.state = "running";
      if (job.kind !== "check" && (conflictsRemaining.get(job.id) ?? 0) > 0) {
        const preference = queueDecisions.get(queueByJob.get(job.id)!);
        if (preference) {
          decide(job, preference);
          if (job.files_done < job.files_total) continue;
        } else {
          const name = `IMG_${7412 + job.files_done}.ARW`;
          job.pending_conflict = {
            request_id: crypto.randomUUID(),
            source_path: `Source/${name}`,
            destination_path: `Destination/${name}`,
            source_hash: "demo-source-hash",
            destination_hash: "demo-different-destination-hash",
          };
          job.current_file = name;
          job.state = "awaiting_decision";
          continue;
        }
      }
      const step = Math.max(1, Math.ceil(job.files_total / 12));
      const n = Math.min(step, job.files_total - job.files_done);
      job.files_done += n;
      job.bytes_done = job.files_done * AVG_FILE;
      if (job.kind === "check")
        job.check_results = checkSummary((checkItemsByJob.get(job.id) ?? []).slice(0, job.files_done));
      job.bytes_per_sec = 180_000_000;
      job.speed_bps = job.bytes_per_sec;
      job.eta_secs = Math.ceil((job.bytes_total - job.bytes_done) / job.bytes_per_sec);
      job.current_file = `IMG_${7412 + job.files_done}.ARW`;
      const c = counts[job.flow_id];
      if (c && job.kind !== "check") {
        c[0] += n;
        c[1] = Math.max(0, c[1] - n);
      }
      if (job.files_done >= job.files_total) {
        job.state = "done";
        job.current_file = null;
        job.eta_secs = 0;
        checkItemsByJob.delete(job.id);
        const flow = snapshot.flows.find((f) => f.id === job.flow_id);
        const dst = snapshot.destinations.find((d) => d.id === flow?.destination_id);
        if (job.kind !== "check" && !skippedJobs.has(job.id) && dst?.device_id && job.bytes_total > 0) {
          snapshot.settings.transfer_speeds ??= {};
          snapshot.settings.transfer_speeds[dst.device_id] = job.bytes_per_sec ?? job.speed_bps;
          snapshot.settings.transfer_speeds._global = job.bytes_per_sec ?? job.speed_bps;
        }
        skippedJobs.delete(job.id);
      }
      for (const [jobId, queue] of queueByJob) {
        if (
          !jobs.some(
            (job) => queueByJob.get(job.id) === queue && !["done", "failed", "cancelled"].includes(job.state),
          )
        ) {
          queueDecisions.delete(queue);
          queueByJob.delete(jobId);
          conflictsRemaining.delete(jobId);
        }
      }
    }
    events.emit("transfers", structuredClone(jobs));
    events.emit("status-changed");
    if (!jobs.some((j) => ["running", "queued", "paused", "awaiting_decision"].includes(j.state))) {
      clearInterval(timer);
      timer = undefined;
    }
  };

  const startFlow = (flowId: string, queue = crypto.randomUUID()): string | undefined => {
    const c = counts[flowId] ?? (counts[flowId] = [0, 25, 0, 0]);
    if (c[3]) {
      c[1] += c[3];
      c[3] = 0;
    }
    const flow = snapshot.flows.find((f) => f.id === flowId);
    const dst = snapshot.destinations.find((d) => d.id === flow?.destination_id);
    const src = snapshot.sources.find((s) => s.id === flow?.source_id);
    if (!src || !snapshot.devices.some((d) => d.id === src.device_id))
      throw new Error("Select a device for this source in its settings");
    if (dst?.kind !== "app" && (!dst || !snapshot.devices.some((d) => d.id === dst.device_id)))
      throw new Error("Select a device for this destination in its settings");
    if (flow) validateProjectConfiguration(snapshot, flow.space_id);
    if (dst?.kind === "app") throw new Error("App destinations can only be triggered manually");
    if (offline.has(src.device_id) || offline.has(dst?.device_id ?? ""))
      throw new Error("Connect the source and destination devices to run");
    if (!c[1]) return;
    const existing = jobs.find(
      (j) => j.flow_id === flowId && !["done", "failed", "cancelled"].includes(j.state),
    );
    if (existing) return existing.id;
    const id = crypto.randomUUID();
    jobs.push({
      id,
      flow_id: flowId,
      label: flow ? flowLabel(snapshot, flow) : "?",
      state: "queued",
      files_done: 0,
      files_total: c[1],
      bytes_done: 0,
      bytes_total: c[1] * AVG_FILE,
      current_file: null,
      speed_bps: 0,
      bytes_per_sec: null,
      eta_secs: null,
      errors: [],
      kind: "transfer",
      pending_conflict: null,
      check_results: null,
    });
    queueByJob.set(id, queue);
    conflictsRemaining.set(id, Math.min(c[1], options.conflicts?.[flowId] ?? 0));
    timer ??= setInterval(tick, options.tickMs ?? 400);
    events.emit("transfers", structuredClone(jobs));
    return id;
  };

  if (options.seedRunningTransfer) {
    startFlow("f4");
    tick();
  }

  const workspaceStatus = (context: WorkspaceContext) =>
    mockWorkspaceStatus(snapshot, context, counts, offline);
  const workspaceFlow = (context: WorkspaceContext, flowId: string) => {
    const status = workspaceStatus(context);
    const flow = snapshot.flows.find((f) => f.id === flowId && f.space_id === context.spaceId);
    if (!flow) throw new Error("Flow must belong to the workspace space");
    const st = status.flows.find((f) => f.flow_id === flowId)!;
    return { flow, status: st };
  };
  const imports = new Map<string, { context: WorkspaceContext; flowId: string; count: number }>();
  const openMockApp = (flowId: string) => {
    const flow = snapshot.flows.find((f) => f.id === flowId);
    const dest = snapshot.destinations.find((d) => d.id === flow?.destination_id);
    const source = snapshot.sources.find((s) => s.id === flow?.source_id);
    if (!source || !snapshot.devices.some((d) => d.id === source.device_id))
      throw new Error("Select a device for this source in its settings");
    if (dest?.kind !== "app") throw new Error("This destination is not an app destination");
    const appPath = snapshot.settings.app_destinations?.[dest.id]?.trim();
    const appName = dest.app_name ?? (appPath ? appPath.split(/[\\/]/).pop() : null) ?? "Application";
    if (!appPath) throw new Error(`Choose the application for ${appName} on this computer`);
    const c = counts[flowId] ?? [0, 0, 0, 0];
    const files = Array.from({ length: c[1] }, (_, i) => ({
      rel_path: `100MSDCF/IMG_${String(7412 + i).padStart(5, "0")}.JPG`,
      project_id: null,
    }));
    console.info(`Demo mode would open ${files.length} files in ${appPath}`);
    return { token: crypto.randomUUID(), app_name: appName, files };
  };

  const backend: Backend = {
    getSnapshot: async () => structuredClone(snapshot),
    saveEntity: async (kind, entity) => {
      const candidate = structuredClone(snapshot);
      const candidateItems = candidate[COLLECTION[kind]] as { id: string }[];
      const candidateIndex = candidateItems.findIndex((e) => e.id === entity.id);
      if (candidateIndex >= 0) candidateItems[candidateIndex] = structuredClone(entity);
      else candidateItems.push(structuredClone(entity));
      if (
        kind === "space" ||
        kind === "project" ||
        kind === "source" ||
        kind === "destination" ||
        kind === "flow"
      ) {
        const spaceId = kind === "space" ? entity.id : "space_id" in entity ? entity.space_id : null;
        if (spaceId) validateProjectConfiguration(candidate, spaceId);
      }
      const items = list(kind);
      const index = items.findIndex((e) => e.id === entity.id);
      if (index >= 0) items[index] = structuredClone(entity);
      else items.push(structuredClone(entity));
      if (kind === "device" && index < 0) offline.add(entity.id);
      changed();
    },
    deleteEntity: async (kind, id) => {
      if (kind === "device") {
        if (!id) throw new Error("device id is required");
        for (const task of [...snapshot.sources, ...snapshot.destinations]) {
          if (task.device_id === id) task.device_id = "";
        }
        snapshot.mappings = snapshot.mappings.filter((m) => m.device_id !== id);
        offline.delete(id);
      }
      const items = list(kind);
      const index = items.findIndex((e) => e.id === id);
      if (index >= 0) items.splice(index, 1);
      if (kind === "source") snapshot.flows = snapshot.flows.filter((f) => f.source_id !== id);
      if (kind === "destination") snapshot.flows = snapshot.flows.filter((f) => f.destination_id !== id);
      if (kind === "project") {
        for (const source of snapshot.sources) {
          if (source.project_scope?.mode === "selected") {
            source.project_scope.project_ids = source.project_scope.project_ids.filter(
              (projectId) => projectId !== id,
            );
          }
        }
      }
      if (kind === "space") {
        for (const key of ["projects", "sources", "destinations", "flows"] as const) {
          (snapshot[key] as { space_id: string }[]) = (snapshot[key] as { space_id: string }[]).filter(
            (e) => e.space_id !== id,
          );
        }
      }
      changed();
    },
    saveSettings: async (settings) => {
      snapshot.settings = structuredClone(settings);
    },
    getProjectStatus: async (projectId) => mockStatus(snapshot, projectId, counts, offline),
    getWorkspaceStatus: async (context) => workspaceStatus(context),
    listWorkspaceFiles: async (req) => {
      const { flow, status } = workspaceFlow(req.context, req.flowId);
      const source = snapshot.sources.find((s) => s.id === flow.source_id)!;
      const space = snapshot.spaces.find((s) => s.id === req.context.spaceId)!;
      const project = snapshot.projects.find((p) => p.id === req.context.projectId) ?? null;
      const vars = previewVars(
        space,
        source.project_scope?.mode === "none" ? null : project,
        sourceBackupName(
          source,
          snapshot.devices.find((d) => d.id === source.device_id),
        ),
      );
      if (templateVars(source.path_template).some((name) => !(name in vars)))
        throw new Error("Source path requires project values or an unknown variable");
      const destination = snapshot.destinations.find((d) => d.id === flow.destination_id)!;
      if (!snapshot.devices.some((d) => d.id === source.device_id))
        throw new Error("Select a device for this source in its settings");
      if (destination.kind !== "app" && !snapshot.devices.some((d) => d.id === destination.device_id))
        throw new Error("Select a device for this destination in its settings");
      const configError = status.state === "error" && !status.runnable ? status.error : null;
      const total = configError
        ? req.category === "error"
          ? status.transferred + status.to_transfer + status.ignored + status.failed
          : 0
        : {
            transferred: status.transferred,
            to_transfer: status.to_transfer,
            ignored: status.ignored,
            error: status.failed,
          }[req.category];
      let targetFolder = expandTemplate(destination.path_template, vars);
      if ((destination.kind ?? "folder") === "folder" && destination.subfolder_per_source) {
        const segment = sanitizeBackupName(
          sourceBackupName(
            source,
            snapshot.devices.find((d) => d.id === source.device_id),
          ),
        );
        targetFolder = [targetFolder.replace(/\/+$/, ""), segment].filter(Boolean).join("/");
      }
      const page = mockFiles(
        total,
        req.category,
        req.offset,
        req.limit,
        req.filter,
        targetFolder,
        destination.rules,
        project ? vars : {},
        req.directory,
        destination.kind === "app",
        configError,
        destination.kind === "app" || destination.preserve_file_structure !== false,
      );
      for (const file of page.items) {
        file.project_id = project?.id ?? null;
        if (configError) {
          file.error = configError;
          file.target_path = null;
        }
      }
      return page;
    },
    listFiles: async ({ projectId, flowId, category, offset, limit, filter, directory }) => {
      const project = snapshot.projects.find((p) => p.id === projectId);
      if (!project) throw new Error(`Project ${projectId} not found`);
      return backend.listWorkspaceFiles({
        context: { spaceId: project.space_id, projectId },
        flowId,
        category,
        offset,
        limit,
        filter,
        directory,
      });
    },
    thumbnail: async () => null,
    getMediaMetadata: async () =>
      ({
        media_type: "raw",
        size_bytes: 48_000_000,
        capture_time: {
          local_datetime: "2026-01-14T09:14:22",
          utc_offset_seconds: null,
          source: "exif_original",
        },
        dimensions: { width: 7008, height: 4672 },
        camera: { make: "Sony", model: "ILCE-7M4", lens_make: "Sony", lens_model: "24-70mm F2.8 GM" },
        exposure: {
          shutter_seconds: 0.004,
          aperture_f_number: 4,
          iso: 400,
          focal_length_mm: 35,
          focal_length_35mm: 35,
          compensation_ev: 0,
        },
        orientation: 1,
        video: null,
      }) satisfies MediaMetadata,
    openMedia: async (absPath) => console.info(`Demo mode would open ${absPath}`),
    revealInFileManager: async (kind, id) => {
      console.info(`Demo mode would reveal ${kind} ${id} in the file manager`);
    },
    openFlowInApp: async (_projectId, flowId) => openMockApp(flowId),
    confirmAppImport: async (_projectId, flowId) => {
      const c = counts[flowId] ?? [0, 0, 0, 0];
      const marked = c[1];
      c[0] += marked;
      c[1] = 0;
      changed();
      return marked;
    },
    openWorkspaceFlowInApp: async (context, flowId) => {
      const { status } = workspaceFlow(context, flowId);
      if (!status.runnable) throw new Error(status.error || "No available files to import");
      const opened = openMockApp(flowId);
      imports.set(opened.token, { context: structuredClone(context), flowId, count: opened.files.length });
      return opened;
    },
    confirmWorkspaceAppImport: async (context, flowId, token) => {
      const session = imports.get(token);
      imports.delete(token);
      if (
        !session ||
        JSON.stringify(session.context) !== JSON.stringify(context) ||
        session.flowId !== flowId
      )
        throw new Error("This app import confirmation does not match the workspace and flow");
      workspaceFlow(context, flowId);
      const c = counts[flowId];
      const marked = Math.min(session.count, c[1]);
      c[0] += marked;
      c[1] -= marked;
      changed();
      return marked;
    },
    checkForUpdate: async () => structuredClone(pendingUpdate),
    installUpdate: async () => {
      if (!pendingUpdate) throw new Error("No pending update. Check for updates before installing.");
      const total = 48_000_000;
      let downloaded = 0;
      while (downloaded < total) {
        await new Promise((resolve) => setTimeout(resolve, 250));
        downloaded = Math.min(total, downloaded + 4_800_000);
        events.emit("update://progress", { downloaded, total });
      }
      await new Promise((resolve) => setTimeout(resolve, 500));
      pendingUpdate = null;
      console.info("Demo mode would restart to finish installing the update");
    },
    runFlow: async (projectId, flowId) => {
      const status = mockStatus(snapshot, projectId, counts, offline).flows.find((f) => f.flow_id === flowId);
      if (!status?.runnable)
        throw new Error(status?.error || "Connect the source and destination devices to run");
      startFlow(flowId);
    },
    runAll: async (projectId) => {
      const status = mockStatus(snapshot, projectId, counts, offline);
      const queue = crypto.randomUUID();
      status.flows
        .filter((f) => f.runnable)
        .filter((f) => {
          const flow = snapshot.flows.find((flow) => flow.id === f.flow_id);
          return snapshot.destinations.find((d) => d.id === flow?.destination_id)?.kind !== "app";
        })
        .forEach((f) => startFlow(f.flow_id, queue));
    },
    runWorkspaceFlow: async (context, flowId) => {
      const { status } = workspaceFlow(context, flowId);
      if (!status.runnable)
        throw new Error(status.error || "Connect the source and destination devices to run");
      startFlow(flowId);
    },
    runWorkspaceAll: async (context) => {
      const queue = crypto.randomUUID();
      for (const st of workspaceStatus(context).flows.filter((f) => f.runnable)) {
        const flow = snapshot.flows.find((f) => f.id === st.flow_id)!;
        if (snapshot.destinations.find((d) => d.id === flow.destination_id)?.kind !== "app")
          startFlow(st.flow_id, queue);
      }
    },
    runWorkspaceDestination: async (context, destinationId) => {
      const destination = snapshot.destinations.find(
        (d) => d.id === destinationId && d.space_id === context.spaceId,
      );
      if (!destination || destination.kind === "app") throw new Error("Select a folder destination");
      const status = workspaceStatus(context);
      const queue = crypto.randomUUID();
      const ids: string[] = [];
      for (const st of status.flows.filter((f) => f.runnable)) {
        if (snapshot.flows.find((f) => f.id === st.flow_id)?.destination_id !== destinationId) continue;
        const id = startFlow(st.flow_id, queue);
        if (id) ids.push(id);
      }
      if (!ids.length) throw new Error("No connected, runnable flows for this destination");
      return ids;
    },
    checkWorkspaceDestination: async (context, destinationId) => {
      const destination = snapshot.destinations.find(
        (d) => d.id === destinationId && d.space_id === context.spaceId,
      );
      if (!destination || destination.kind === "app") throw new Error("Select a folder destination");
      const status = workspaceStatus(context);
      if (!status.destinations.find((d) => d.destination_id === destinationId)?.available)
        throw new Error("Connect the destination device to check");
      const ids: string[] = [];
      for (const flow of snapshot.flows.filter(
        (f) => f.space_id === context.spaceId && f.destination_id === destinationId,
      )) {
        const st = status.flows.find((f) => f.flow_id === flow.id)!;
        if (
          !status.sources.find((s) => s.source_id === flow.source_id)?.available ||
          st.state === "unavailable" ||
          (st.state === "error" && !st.runnable)
        )
          continue;
        const total = st.transferred + st.to_transfer + st.failed;
        if (!total) continue;
        const key = `check:${flow.id}`;
        const existing = jobs.find(
          (j) => j.flow_id === key && !["done", "failed", "cancelled"].includes(j.state),
        );
        if (existing) {
          ids.push(existing.id);
          continue;
        }
        const items: DestinationCheckItem[] = [];
        for (const [category, outcome] of [
          ["transferred", "matched"],
          ["to_transfer", "missing"],
          ["error", "conflict"],
        ] as const) {
          const page = await backend.listWorkspaceFiles({
            context,
            flowId: flow.id,
            category,
            offset: 0,
            limit: 1000,
          });
          for (let offset = 0; offset < page.total; offset += 1000) {
            const rows =
              offset === 0
                ? page
                : await backend.listWorkspaceFiles({
                    context,
                    flowId: flow.id,
                    category,
                    offset,
                    limit: 1000,
                  });
            items.push(
              ...rows.items.map((file) => ({
                source_path: file.rel_path,
                destination_path: file.target_path ?? file.rel_path,
                outcome,
                error: null,
              })),
            );
          }
        }
        const id = crypto.randomUUID();
        jobs.push({
          id,
          flow_id: key,
          label: `Check ${flowLabel(snapshot, flow)}`,
          kind: "check",
          state: "queued",
          files_done: 0,
          files_total: items.length,
          bytes_done: 0,
          bytes_total: items.length * AVG_FILE,
          current_file: null,
          speed_bps: 0,
          bytes_per_sec: null,
          eta_secs: null,
          errors: [],
          pending_conflict: null,
          check_results: checkSummary([]),
        });
        checkItemsByJob.set(id, items);
        ids.push(id);
      }
      if (!ids.length) throw new Error("No connected, eligible files for this destination");
      timer ??= setInterval(tick, options.tickMs ?? 400);
      events.emit("transfers", structuredClone(jobs));
      return ids;
    },
    resolveTransferConflict: async (jobId, requestId, decision, applyToRemaining) => {
      const job = jobs.find((j) => j.id === jobId);
      if (
        !job?.pending_conflict ||
        job.pending_conflict.request_id !== requestId ||
        ["done", "failed", "cancelled"].includes(job.state)
      )
        throw new Error("This conflict request is no longer pending");
      const queue = queueByJob.get(jobId)!;
      if (applyToRemaining) {
        queueDecisions.set(queue, decision);
        for (const waiting of jobs.filter((j) => queueByJob.get(j.id) === queue && j.pending_conflict))
          decide(waiting, decision);
      } else decide(job, decision);
      timer ??= setInterval(tick, options.tickMs ?? 400);
      events.emit("transfers", structuredClone(jobs));
    },
    setTransferPaused: async (jobId, isPaused) => {
      const job = jobs.find((j) => j.id === jobId);
      if (!job) return;
      if (isPaused) paused.add(jobId);
      else paused.delete(jobId);
      if (job.state === "running" || job.state === "paused" || job.state === "queued")
        if (!job.pending_conflict) job.state = isPaused ? "paused" : "running";
      events.emit("transfers", structuredClone(jobs));
    },
    setAllPaused: async (isPaused) => {
      for (const job of jobs) {
        if (["done", "failed", "cancelled"].includes(job.state)) continue;
        if (isPaused) paused.add(job.id);
        else paused.delete(job.id);
        job.state = isPaused ? "paused" : "running";
      }
      events.emit("transfers", structuredClone(jobs));
    },
    cancelTransfer: async (jobId) => {
      const job = jobs.find((j) => j.id === jobId);
      if (job) {
        job.state = "cancelled";
        job.pending_conflict = null;
        skippedJobs.delete(job.id);
        if (job.kind === "check" && job.check_results) {
          job.check_results = checkSummary((checkItemsByJob.get(job.id) ?? []).slice(0, job.files_done));
          checkItemsByJob.delete(job.id);
        }
      }
      events.emit("transfers", structuredClone(jobs));
    },
    listTransfers: async () => structuredClone(jobs),
    listVolumes: async (): Promise<Volume[]> =>
      [
        {
          mount_path: "/Volumes/CAM_A_01",
          name: "CAM_A_01",
          volume_uuid: "4F2A-91C3",
          hw_serial: null,
          total_bytes: 128e9,
          free_bytes: 70e9,
          removable: true,
          device_id: "card1",
          matched_by: "marker",
        },
        {
          mount_path: "/Volumes/CAM_A_02",
          name: "CAM_A_02",
          volume_uuid: "77B1-02AA",
          hw_serial: null,
          total_bytes: 128e9,
          free_bytes: 80e9,
          removable: true,
          device_id: "card2",
          matched_by: "marker",
        },
        {
          mount_path: "/Volumes/Untitled",
          name: "Untitled",
          volume_uuid: "9C0D-1E2F",
          hw_serial: null,
          total_bytes: 64e9,
          free_bytes: 60e9,
          removable: true,
          device_id: null,
          matched_by: null,
        },
      ].map((volume): Volume => {
        const mapping = snapshot.mappings.find(
          (m) =>
            m.computer_id === snapshot.computer.id &&
            m.root_path === volume.mount_path &&
            snapshot.devices.some((d) => d.id === m.device_id),
        );
        return { ...volume, device_id: mapping?.device_id ?? null, matched_by: mapping ? "mapping" : null };
      }),
    registerDevice: async (mountPath, device) => {
      const index = snapshot.devices.findIndex((d) => d.id === device.id);
      if (index < 0) snapshot.devices.push(structuredClone(device));
      else snapshot.devices[index] = structuredClone(device);
      const mapping = {
        id: `${device.id}@${snapshot.computer.id}`,
        device_id: device.id,
        computer_id: snapshot.computer.id,
        root_path: mountPath,
      };
      snapshot.mappings = snapshot.mappings.filter((m) => m.id !== mapping.id);
      snapshot.mappings.push(mapping);
      offline.delete(device.id);
      changed();
    },
    relinkDevice: async (deviceId, mountPath) => {
      if (!snapshot.devices.some((d) => d.id === deviceId)) throw new Error(`Device ${deviceId} not found`);
      const mapping = snapshot.mappings.find(
        (m) => m.device_id === deviceId && m.computer_id === snapshot.computer.id,
      );
      if (mapping) mapping.root_path = mountPath;
      else
        snapshot.mappings.push({
          id: `${deviceId}@${snapshot.computer.id}`,
          device_id: deviceId,
          computer_id: snapshot.computer.id,
          root_path: mountPath,
        });
      offline.delete(deviceId);
      changed();
    },
    pickFolder: async () => window.prompt("Folder path (demo mode)", "/Volumes/Untitled") ?? null,
    pickPreviewApp: async () =>
      window.prompt("Preview app path (demo mode)", "/Applications/Preview.app") ?? null,
    planWipe: async (projectId, sourceId) => {
      const status = mockStatus(snapshot, projectId, counts, offline).sources.find(
        (s) => s.source_id === sourceId,
      );
      const finals = snapshot.devices.filter((d) => d.role === "final");
      return {
        source_id: sourceId,
        files_total: status?.file_count ?? 0,
        ignored: 36,
        copies: finals.map((d) => ({
          device_id: d.id,
          device_name: d.name,
          verified: status?.wipe_eligible ? (status.file_count ?? 0) : 0,
          total: status?.file_count ?? 0,
        })),
        eligible: status?.wipe_eligible ?? false,
        reason: status?.blocking_reason ?? null,
      };
    },
    wipe: async (projectId, sourceId) => {
      const status = mockStatus(snapshot, projectId, counts, offline).sources.find(
        (s) => s.source_id === sourceId,
      );
      if (!status?.wipe_eligible) throw new Error(status?.blocking_reason || "Not safe to wipe this source");
      const source = snapshot.sources.find((s) => s.id === sourceId);
      snapshot.flows.filter((f) => f.source_id === sourceId).forEach((f) => (counts[f.id] = [0, 0, 0, 0]));
      if (source) source.offer_wipe = true;
      changed();
    },
    syncStatus: async () => demoSync(),
    addPeer: async () => events.emit("sync-status", demoSync()),
    removePeer: async () => events.emit("sync-status", demoSync()),
    syncNow: async () => events.emit("sync-status", demoSync()),
    on: async (event, handler) => events.on(event, handler as never),
  };
  return backend;
}
