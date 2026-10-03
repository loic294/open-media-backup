import { describe, expect, it } from "vitest";
import { createMockBackend } from "../api/mock/mock-backend";
import { AppStore } from "../state/app-store";
import { handleDesktopMenuAction } from "./menu";

async function setup() {
  const store = new AppStore(createMockBackend({ tickMs: 5 }));
  await store.init();
  return store;
}

describe("desktop menu actions", () => {
  it("opens app settings", async () => {
    const store = await setup();
    await handleDesktopMenuAction(store, { action: "open-settings" });
    expect(store.dialogs).toEqual([{ type: "app-settings" }]);
    store.dispose();
  });

  it("switches spaces by id", async () => {
    const store = await setup();
    await handleDesktopMenuAction(store, { action: "switch-space", spaceId: "home" });
    expect(store.space?.id).toBe("home");
    store.dispose();
  });

  it("opens settings for the active space", async () => {
    const store = await setup();
    await handleDesktopMenuAction(store, { action: "switch-space", spaceId: "backup" });
    await handleDesktopMenuAction(store, { action: "space-settings" });
    expect(store.dialogs).toEqual([{ type: "space-settings", spaceId: "backup" }]);
    store.dispose();
  });
});
