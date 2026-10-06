import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockBackend } from "../../api/mock/mock-backend";
import { DEMO_HASH_ROOT_ID, DEMO_HASH_SERVER_ID } from "../../api/mock/hash-servers";
import { store } from "../../state";
import { OmbDestinationDialog } from "./destination-dialog";
import { OmbSyncDialog } from "./sync-dialog";
import type { OmbHashServersSection } from "./hash-servers-section";
import type { OmbRemoteHashField } from "../form/remote-hash-field";

describe("hash-server dialogs", () => {
  const oldSnapshot = store.snapshot;
  const oldServers = store.hashServers;
  const oldBackend = { ...store.backend };
  let dialog: OmbDestinationDialog | OmbSyncDialog;

  beforeEach(async () => {
    Object.assign(store.backend, createMockBackend());
    store.snapshot = await store.backend.getSnapshot();
    store.hashServers = [];
  });
  afterEach(() => {
    dialog?.remove();
    store.snapshot = oldSnapshot;
    store.hashServers = oldServers;
    Object.assign(store.backend, oldBackend);
    vi.restoreAllMocks();
  });

  async function mountDestination() {
    dialog = new OmbDestinationDialog();
    dialog.request = { type: "destination-settings", destinationId: "d1" };
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
    const field = dialog.querySelector<OmbRemoteHashField>("omb-remote-hash-field")!;
    await field.updateComplete;
    return field;
  }

  it("adds and tests a local server in Device sync, masking the token", async () => {
    dialog = new OmbSyncDialog();
    dialog.request = { type: "device-sync" };
    document.body.append(dialog);
    await vi.waitFor(() => expect(dialog.textContent).toContain("No hash servers added"));
    const address = dialog.querySelector<HTMLInputElement>('[aria-label="Hash server address"]')!;
    const token = dialog.querySelector<HTMLInputElement>('[aria-label="Hash server token"]')!;
    expect(token.type).toBe("password");
    address.value = "nas:47822";
    address.dispatchEvent(new Event("input"));
    token.value = "demo-token";
    token.dispatchEvent(new Event("input"));
    await dialog.querySelector<OmbHashServersSection>("omb-hash-servers-section")!.updateComplete;
    [...dialog.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent?.trim() === "Add hash server")!
      .click();
    await vi.waitFor(() => expect(dialog.textContent).toContain("NAS hash server (demo)"));
    expect(store.hashServers).toHaveLength(1);
    expect(token.value).toBe("");
    const roots = vi.spyOn(store.backend, "hashServerRoots");
    [...dialog.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent?.trim() === "Test")!
      .click();
    await vi.waitFor(() => expect(roots).toHaveBeenCalledWith(DEMO_HASH_SERVER_ID));
  });

  it("persists enabled mapping only, browses subfolders, and does not test unsaved changes", async () => {
    await store.backend.addHashServer("nas:47822", "demo-token");
    await store.refreshHashServers();
    const save = vi.spyOn(store.backend, "saveEntity");
    const field = await mountDestination();
    const toggle = field.querySelector<HTMLInputElement>('input[aria-label="Remote hash check"]')!;
    expect(toggle.checked).toBe(false);
    toggle.click();
    await dialog.updateComplete;
    await field.updateComplete;
    const select = field.querySelector<HTMLSelectElement>('[aria-label="Hash server"]')!;
    select.value = DEMO_HASH_SERVER_ID;
    select.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(field.textContent).toContain("/data/photos"));
    const folder = field.querySelector<HTMLSelectElement>('[aria-label="Remote device-root folder"]')!;
    folder.value = DEMO_HASH_ROOT_ID;
    folder.dispatchEvent(new Event("change"));
    await dialog.updateComplete;
    await field.updateComplete;
    const test = [...field.querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent?.trim() === "Test mapping",
    )!;
    expect(test.disabled).toBe(true);
    [...field.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent?.trim() === "Browse subfolders")!
      .click();
    await vi.waitFor(() =>
      expect([...field.querySelectorAll("button")].some((b) => b.textContent?.trim() === "Archive")).toBe(
        true,
      ),
    );
    [...field.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent?.trim() === "Archive")!
      .click();
    await dialog.updateComplete;
    await field.updateComplete;
    expect(field.textContent).toContain("/data/photos/Archive");
    [...dialog.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent?.trim() === "Save")!
      .click();
    await vi.waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        "destination",
        expect.objectContaining({
          remote_hash: {
            server_id: DEMO_HASH_SERVER_ID,
            root: `${DEMO_HASH_ROOT_ID}/Archive`,
            enabled: true,
          },
        }),
      ),
    );
    expect(JSON.stringify(save.mock.calls)).not.toContain("demo-token");
  });

  it("preserves an unknown synced mapping and allows disabling it", async () => {
    const destination = store.snapshot!.destinations.find((d) => d.id === "d1")!;
    destination.remote_hash = { server_id: "unknown-server", root: DEMO_HASH_ROOT_ID, enabled: true };
    const save = vi.spyOn(store.backend, "saveEntity");
    const field = await mountDestination();
    expect(field.textContent).toContain("not available on this computer");
    expect(field.textContent).toContain("local re-reads");
    const toggle = field.querySelector<HTMLInputElement>('input[aria-label="Remote hash check"]')!;
    toggle.click();
    await dialog.updateComplete;
    await field.updateComplete;
    [...dialog.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent?.trim() === "Save")!
      .click();
    await vi.waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        "destination",
        expect.objectContaining({
          remote_hash: { server_id: "unknown-server", root: DEMO_HASH_ROOT_ID, enabled: false },
        }),
      ),
    );
  });

  it("surfaces root-list failure without replacing a saved mapping", async () => {
    await store.backend.addHashServer("nas:47822", "demo-token");
    await store.refreshHashServers();
    store.snapshot!.destinations.find((d) => d.id === "d1")!.remote_hash = {
      server_id: DEMO_HASH_SERVER_ID,
      root: DEMO_HASH_ROOT_ID,
      enabled: true,
    };
    vi.spyOn(store.backend, "hashServerRoots").mockRejectedValue(new Error("NAS is offline"));
    const test = vi
      .spyOn(store.backend, "testRemoteHashMapping")
      .mockRejectedValue(new Error("hashes differ"));
    const toast = vi.spyOn(store, "toast");
    const field = await mountDestination();
    await vi.waitFor(() => expect(field.textContent).toContain("NAS is offline"));
    [...field.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent?.trim() === "Test mapping")!
      .click();
    await vi.waitFor(() => expect(test).toHaveBeenCalledWith("d1"));
    expect(toast).toHaveBeenCalledWith("error", "Error: hashes differ");
    expect(store.snapshot!.destinations.find((d) => d.id === "d1")?.remote_hash?.root).toBe(
      DEMO_HASH_ROOT_ID,
    );
  });
});
