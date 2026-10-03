import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { inDesktopShell } from "../api";
import type { Space } from "../api/types";
import type { AppStore } from "../state/app-store";
import { activeSpace, sortedSpaces } from "../state/selectors";

const MENU_EVENT = "menu://action";

export type DesktopMenuAction =
  | { action: "open-settings" }
  | { action: "switch-space"; spaceId?: string }
  | { action: "space-settings"; spaceId?: string };

export async function handleDesktopMenuAction(store: AppStore, payload: DesktopMenuAction): Promise<void> {
  switch (payload.action) {
    case "open-settings":
      store.open({ type: "app-settings" });
      return;
    case "switch-space":
      if (payload.spaceId && store.snapshot?.spaces.some((space) => space.id === payload.spaceId)) {
        await store.selectSpace(payload.spaceId);
      }
      return;
    case "space-settings": {
      const spaceId = payload.spaceId ?? store.space?.id;
      if (spaceId && store.snapshot?.spaces.some((space) => space.id === spaceId)) {
        store.open({ type: "space-settings", spaceId });
      }
      return;
    }
  }
}

export async function connectDesktopMenu(store: AppStore): Promise<() => void> {
  if (!inDesktopShell()) return () => {};

  let lastSignature = "";
  let updateQueue = Promise.resolve();
  const sync = () => {
    if (!store.snapshot) return;
    const spaces = sortedSpaces(store.snapshot).map(menuSpace);
    const activeId = activeSpace(store.snapshot)?.id ?? null;
    const signature = JSON.stringify({ spaces, activeId });
    if (signature === lastSignature) return;
    lastSignature = signature;
    updateQueue = updateQueue.finally(() =>
      invoke("set_spaces_menu", { spaces, activeId }).catch((error) => {
        lastSignature = "";
        console.error("Could not update Spaces menu", error);
      }),
    );
  };

  const unlistenMenu = await listen<DesktopMenuAction>(MENU_EVENT, (event) => {
    void handleDesktopMenuAction(store, event.payload);
  });
  store.addEventListener("change", sync);
  sync();

  return () => {
    unlistenMenu();
    store.removeEventListener("change", sync);
  };
}

function menuSpace(space: Space): Pick<Space, "id" | "name"> {
  return { id: space.id, name: space.name };
}
