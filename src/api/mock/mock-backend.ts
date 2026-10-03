import type { Backend } from "../backend";
import type {
  EntityKind,
  MediaMetadata,
  Project,
  Snapshot,
  Space,
  TransferJob,
  UpdateInfo,
  Volume,
} from "../types";
import { demoCounts, demoOffline, demoSnapshot } from "./data";
import { Emitter } from "./emitter";
import { mockFiles } from "./files";
import { mockStatus, type Counts } from "./status";
import { demoSync } from "./sync";
import { validateProjectConfiguration } from "../../state/projects";

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

function projectVars(space: Space | undefined, project: Project | undefined): Record<string, string> {
  if (!space || !project) return {};
  const vars: Record<string, string> = {
    project: project.name,
    project_name: project.name,
  };
  for (const variable of space.variables) {
    if (variable.default_value) vars[variable.name] = variable.default_value;
  }
  for (const [name, value] of Object.entries(project.values)) {
    if (value) vars[name] = value;
  }
  return vars;
}

/** In-memory backend used in a plain browser (and in UI tests). Simulates transfers. */
export function createMockBackend(
  options: { tickMs?: number; seedRunningTransfer?: boolean; demoUpdate?: boolean } = {},
): Backend {
  const snapshot = demoSnapshot();
  const counts: Counts = structuredClone(demoCounts);
  const offline = new Set(demoOffline);
  const events = new Emitter();
  const jobs: TransferJob[] = [];
  const paused = new Set<string>();
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

  const tick = () => {
    for (const job of jobs.filter((j) => j.state === "running" || j.state === "queued")) {
      if (paused.has(job.id)) continue;
      job.state = "running";
      const step = Math.max(1, Math.ceil(job.files_total / 12));
      const n = Math.min(step, job.files_total - job.files_done);
      job.files_done += n;
      job.bytes_done = job.files_done * AVG_FILE;
      job.bytes_per_sec = 180_000_000;
      job.speed_bps = job.bytes_per_sec;
      job.eta_secs = Math.ceil((job.bytes_total - job.bytes_done) / job.bytes_per_sec);
      job.current_file = `IMG_${7412 + job.files_done}.ARW`;
      const c = counts[job.flow_id];
      if (c) {
        c[0] += n;
        c[1] = Math.max(0, c[1] - n);
      }
      if (job.files_done >= job.files_total) {
        job.state = "done";
        job.current_file = null;
        job.eta_secs = 0;
        const flow = snapshot.flows.find((f) => f.id === job.flow_id);
        const dst = snapshot.destinations.find((d) => d.id === flow?.destination_id);
        if (dst?.device_id && job.bytes_total > 0) {
          snapshot.settings.transfer_speeds ??= {};
          snapshot.settings.transfer_speeds[dst.device_id] = job.bytes_per_sec ?? job.speed_bps;
          snapshot.settings.transfer_speeds._global = job.bytes_per_sec ?? job.speed_bps;
        }
      }
    }
    events.emit("transfers", structuredClone(jobs));
    events.emit("status-changed");
    if (!jobs.some((j) => j.state === "running" || j.state === "queued")) {
      clearInterval(timer);
      timer = undefined;
    }
  };

  const startFlow = (flowId: string) => {
    const c = counts[flowId] ?? (counts[flowId] = [0, 25, 0, 0]);
    if (c[3]) {
      c[1] += c[3];
      c[3] = 0;
    }
    const flow = snapshot.flows.find((f) => f.id === flowId);
    const name = (id?: string) => snapshot.devices.find((d) => d.id === id)?.name ?? "?";
    const src = snapshot.sources.find((s) => s.id === flow?.source_id);
    const dst = snapshot.destinations.find((d) => d.id === flow?.destination_id);
    if (flow) validateProjectConfiguration(snapshot, flow.space_id);
    if (dst?.kind === "app") throw new Error("App destinations can only be triggered manually");
    if (!c[1] || offline.has(dst?.device_id ?? "")) return;
    if (jobs.some((j) => j.flow_id === flowId && !["done", "failed", "cancelled"].includes(j.state))) return;
    jobs.push({
      id: crypto.randomUUID(),
      flow_id: flowId,
      label: `${name(src?.device_id)} → ${name(dst?.device_id)}`,
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
    });
    timer ??= setInterval(tick, options.tickMs ?? 400);
  };

  if (options.seedRunningTransfer) {
    startFlow("f4");
    tick();
  }

  return {
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
      changed();
    },
    deleteEntity: async (kind, id) => {
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
    listFiles: async ({ projectId, flowId, category, offset, limit, filter }) => {
      const [transferred, toTransfer, ignored, failed] = counts[flowId] ?? [0, 25, 0, 0];
      const total = { transferred, to_transfer: toTransfer, ignored, error: failed }[category];
      const project = snapshot.projects.find((p) => p.id === projectId);
      const space = snapshot.spaces.find((s) => s.id === project?.space_id);
      const dest = snapshot.destinations.find(
        (d) => d.id === snapshot.flows.find((f) => f.id === flowId)?.destination_id,
      );
      return mockFiles(
        total,
        category,
        offset,
        limit,
        filter,
        dest?.path_template ?? "",
        dest?.rules ?? [],
        projectVars(space, project),
      );
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
    openFlowInApp: async (_projectId, flowId) => {
      const flow = snapshot.flows.find((f) => f.id === flowId);
      const dest = snapshot.destinations.find((d) => d.id === flow?.destination_id);
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
    },
    confirmAppImport: async (_projectId, flowId) => {
      const c = counts[flowId] ?? [0, 0, 0, 0];
      const marked = c[1];
      c[0] += marked;
      c[1] = 0;
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
    runFlow: async (_projectId, flowId) => startFlow(flowId),
    runAll: async (projectId) => {
      const status = mockStatus(snapshot, projectId, counts, offline);
      status.flows
        .filter((f) => f.state === "pending" || f.state === "error")
        .filter((f) => {
          const flow = snapshot.flows.find((flow) => flow.id === f.flow_id);
          return snapshot.destinations.find((d) => d.id === flow?.destination_id)?.kind !== "app";
        })
        .forEach((f) => startFlow(f.flow_id));
    },
    setTransferPaused: async (jobId, isPaused) => {
      const job = jobs.find((j) => j.id === jobId);
      if (!job) return;
      if (isPaused) paused.add(jobId);
      else paused.delete(jobId);
      if (job.state === "running" || job.state === "paused" || job.state === "queued")
        job.state = isPaused ? "paused" : "running";
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
      if (job) job.state = "cancelled";
      events.emit("transfers", structuredClone(jobs));
    },
    listTransfers: async () => structuredClone(jobs),
    listVolumes: async (): Promise<Volume[]> => [
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
    ],
    registerDevice: async (mountPath, device) => {
      list("device").push(structuredClone(device));
      snapshot.mappings.push({
        id: `${device.id}@${snapshot.computer.id}`,
        device_id: device.id,
        computer_id: snapshot.computer.id,
        root_path: mountPath,
      });
      changed();
    },
    relinkDevice: async (deviceId, mountPath) => {
      const mapping = snapshot.mappings.find(
        (m) => m.device_id === deviceId && m.computer_id === snapshot.computer.id,
      );
      if (mapping) mapping.root_path = mountPath;
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
    wipe: async (_projectId, sourceId) => {
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
}
