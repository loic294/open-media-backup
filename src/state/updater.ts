import type { UpdateProgress } from "../api/types";
import type { AppStore } from "./app-store";

let checked = false;

export async function checkForUpdateOnLaunch(store: AppStore): Promise<void> {
  if (checked) return;
  checked = true;
  try {
    store.setAvailableUpdate(await store.backend.checkForUpdate());
  } catch (error) {
    console.warn("Update check failed", error);
  }
}

export async function installAvailableUpdate(
  store: AppStore,
  onProgress: (progress: UpdateProgress) => void,
): Promise<void> {
  const unlisten = await store.backend.on("update://progress", onProgress);
  try {
    await store.backend.installUpdate();
  } finally {
    unlisten();
  }
}
