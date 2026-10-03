import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Backend } from "./backend";

const thumbnails = new Map<string, Promise<string | null>>();

/** Thumbnails arrive as raw JPEG bytes; each becomes a blob URL, cached for the session. */
function thumbnailUrl(absPath: string): Promise<string | null> {
  let url = thumbnails.get(absPath);
  if (!url) {
    url = invoke<ArrayBuffer>("thumbnail", { absPath }).then((bytes) =>
      bytes.byteLength ? URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" })) : null,
    );
    url.catch(() => thumbnails.delete(absPath));
    thumbnails.set(absPath, url);
  }
  return url;
}

/** Thin typed wrapper over Tauri commands (see src-tauri/src/commands). */
export const tauriBackend: Backend = {
  getSnapshot: () => invoke("get_snapshot"),
  saveEntity: (kind, entity) => invoke("save_entity", { kind, entity }),
  deleteEntity: (kind, id) => invoke("delete_entity", { kind, id }),
  saveSettings: (settings) => invoke("save_settings", { settings }),

  getProjectStatus: (projectId) => invoke("get_project_status", { projectId }),
  listFiles: (req) => invoke("list_files", { req }),
  thumbnail: (absPath) => thumbnailUrl(absPath),

  runFlow: (projectId, flowId) => invoke("run_flow", { projectId, flowId }),
  runAll: (projectId) => invoke("run_all", { projectId }),
  setTransferPaused: (jobId, paused) => invoke("set_transfer_paused", { jobId, paused }),
  setAllPaused: (paused) => invoke("set_all_paused", { paused }),
  cancelTransfer: (jobId) => invoke("cancel_transfer", { jobId }),
  listTransfers: () => invoke("list_transfers"),

  listVolumes: () => invoke("list_volumes"),
  registerDevice: (mountPath, device) => invoke("register_device", { mountPath, device }),
  relinkDevice: (deviceId, mountPath) => invoke("relink_device", { deviceId, mountPath }),
  pickFolder: async (defaultPath) => {
    const picked = await open({ directory: true, defaultPath });
    return typeof picked === "string" ? picked : null;
  },

  planWipe: (projectId, sourceId) => invoke("plan_wipe", { projectId, sourceId }),
  wipe: (projectId, sourceId, method) => invoke("wipe", { projectId, sourceId, method }),

  syncStatus: () => invoke("sync_status"),
  addPeer: (address, token) => invoke("add_peer", { address, token }),
  removePeer: (peerId) => invoke("remove_peer", { peerId }),
  syncNow: () => invoke("sync_now"),

  on: (event, handler) => listen(event, (e) => handler(e.payload as never)),
};
