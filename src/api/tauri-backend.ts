import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Backend } from "./backend";
import type { Snapshot } from "./types";
import { appPickerOptions } from "../utils/app-picker";

const thumbnails = new Map<string, Promise<string | null>>();

/**
 * Raw IPC responses are an ArrayBuffer over the custom protocol, but a plain number array when
 * Tauri falls back to postMessage (always on macOS for that path).
 */
export function jpegBytes(payload: ArrayBuffer | Uint8Array | number[] | null | undefined): Uint8Array<ArrayBuffer> {
  if (!payload) return new Uint8Array();
  if (payload instanceof ArrayBuffer) return new Uint8Array(payload);
  if (Array.isArray(payload)) return Uint8Array.from(payload);
  return Uint8Array.from(payload);
}

/** Thumbnails arrive as raw JPEG bytes; each becomes a blob URL, cached for the session. */
function thumbnailUrl(absPath: string): Promise<string | null> {
  let url = thumbnails.get(absPath);
  if (!url) {
    url = invoke<ArrayBuffer | Uint8Array | number[]>("thumbnail", { absPath }).then((payload) => {
      const bytes = jpegBytes(payload);
      return bytes.byteLength ? URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" })) : null;
    });
    url.catch(() => thumbnails.delete(absPath));
    thumbnails.set(absPath, url);
  }
  return url;
}

let cameraThumbnailRules: string | undefined;

/** Drops cached thumbnails when the camera thumbnail paths change so new rules apply immediately. */
function noteCameraThumbnailRules(paths: string[] | undefined) {
  const rules = JSON.stringify(paths ?? null);
  if (cameraThumbnailRules !== undefined && cameraThumbnailRules !== rules) {
    for (const url of thumbnails.values()) url.then((u) => u && URL.revokeObjectURL(u)).catch(() => {});
    thumbnails.clear();
  }
  cameraThumbnailRules = rules;
}

/** Thin typed wrapper over Tauri commands (see src-tauri/src/commands). */
export const tauriBackend: Backend = {
  getSnapshot: async () => {
    const snapshot = await invoke<Snapshot>("get_snapshot");
    noteCameraThumbnailRules(snapshot.settings.camera_thumbnail_paths);
    return snapshot;
  },
  saveEntity: (kind, entity) => invoke("save_entity", { kind, entity }),
  deleteEntity: (kind, id) => invoke("delete_entity", { kind, id }),
  saveSettings: async (settings) => {
    await invoke("save_settings", { settings });
    noteCameraThumbnailRules(settings.camera_thumbnail_paths);
  },

  getProjectStatus: (projectId) => invoke("get_project_status", { projectId }),
  getWorkspaceStatus: (context) => invoke("get_workspace_status", { context }),
  getSourceSafeCopyDetails: (context, sourceId) =>
    invoke("get_source_safe_copy_details", { context, sourceId }),
  saveDeviceSafeCopyRules: (context, deviceId, rules) =>
    invoke("save_device_safe_copy_rules", { context, deviceId, rules }),
  listWorkspaceFiles: (req) => invoke("list_workspace_files", { req }),
  listFiles: (req) => invoke("list_files", { req }),
  thumbnail: (absPath) => thumbnailUrl(absPath),
  getMediaMetadata: (absPath) => invoke("get_media_metadata", { absPath }),
  openMedia: (absPath) => invoke("open_media_file", { absPath }),
  revealInFileManager: (kind, id) => invoke("reveal_in_file_manager", { kind, id }),
  openFlowInApp: (projectId, flowId) => invoke("open_flow_in_app", { projectId, flowId }),
  confirmAppImport: (projectId, flowId, token) => invoke("confirm_app_import", { projectId, flowId, token }),
  openWorkspaceFlowInApp: (context, flowId) => invoke("open_workspace_flow_in_app", { context, flowId }),
  confirmWorkspaceAppImport: (context, flowId, token) =>
    invoke("confirm_workspace_app_import", { context, flowId, token }),
  checkForUpdate: () => invoke("check_for_update"),
  installUpdate: () => invoke("install_update"),

  runFlow: (projectId, flowId) => invoke("run_flow", { projectId, flowId }),
  runAll: (projectId) => invoke("run_all", { projectId }),
  runWorkspaceFlow: (context, flowId) => invoke("run_workspace_flow", { context, flowId }),
  runWorkspaceAll: (context) => invoke("run_workspace_all", { context }),
  runWorkspaceDestination: (context, destinationId) =>
    invoke("run_workspace_destination", { context, destinationId }),
  checkWorkspaceDestination: (context, destinationId, scope = { kind: "configuredSources" }) =>
    invoke("check_workspace_destination", { context, destinationId, scope }),
  resolveTransferConflict: (jobId, requestId, decision, applyToRemaining) =>
    invoke("resolve_transfer_conflict", { jobId, requestId, decision, applyToRemaining }),
  setTransferPaused: (jobId, paused) => invoke("set_transfer_paused", { jobId, paused }),
  setAllPaused: (paused) => invoke("set_all_paused", { paused }),
  cancelTransfer: (jobId) => invoke("cancel_transfer", { jobId }),
  listTransfers: () => invoke("list_transfers"),
  getSpeedAnalysis: (req) => invoke("get_speed_analysis", { req }),
  listSpeedAnalysisJobs: (req) => invoke("list_speed_analysis_jobs", { req }),

  listVolumes: () => invoke("list_volumes"),
  registerDevice: (mountPath, device) => invoke("register_device", { mountPath, device }),
  relinkDevice: (deviceId, mountPath) => invoke("relink_device", { deviceId, mountPath }),
  pickFolder: async (defaultPath) => {
    const picked = await open({ directory: true, defaultPath });
    return typeof picked === "string" ? picked : null;
  },
  pickPreviewApp: async (os) => {
    const picked = await open(appPickerOptions(os));
    return typeof picked === "string" ? invoke("validate_app_path", { appPath: picked }) : null;
  },

  planWipe: (sourceId) => invoke("plan_wipe", { sourceId }),
  wipe: (sourceId, method) => invoke("wipe", { sourceId, method }),
  markSourceManuallyWiped: (sourceId) => invoke("mark_source_manually_wiped", { sourceId }),

  syncStatus: () => invoke("sync_status"),
  addPeer: (address, token) => invoke("add_peer", { address, token }),
  removePeer: (peerId) => invoke("remove_peer", { peerId }),
  syncNow: () => invoke("sync_now"),
  listHashServers: () => invoke("list_hash_servers"),
  addHashServer: (address, token) => invoke("add_hash_server", { address, token }),
  removeHashServer: (id) => invoke("remove_hash_server", { id }),
  hashServerRoots: (id) => invoke("hash_server_roots", { id }),
  hashServerBrowse: (id, root, path) => invoke("hash_server_browse", { id, root, path }),
  testRemoteHashMapping: (destinationId) => invoke("test_remote_hash_mapping", { destinationId }),

  on: (event, handler) => listen(event, (e) => handler(e.payload as never)),
};
