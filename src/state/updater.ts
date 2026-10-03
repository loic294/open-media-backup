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
