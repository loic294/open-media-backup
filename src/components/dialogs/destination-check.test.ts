import { afterEach, describe, expect, it, vi } from "vitest";
import type { TransferJob } from "../../api/types";
import { store } from "../../state";
import { OmbTransferConflictDialog } from "./transfer-conflict-dialog";
import { OmbDestinationCheckResultsDialog } from "./destination-check-results-dialog";
import { OmbDialogHost } from "./omb-dialog-host";

const job: TransferJob = {
  id: "j",
  flow_id: "f",
  label: "Camera to NAS",
  kind: "transfer",
  state: "awaiting_decision",
  files_done: 0,
  files_total: 1,
  bytes_done: 0,
  bytes_total: 4,
  current_file: "photo.jpg",
  speed_bps: 0,
  bytes_per_sec: null,
  eta_secs: null,
  errors: [],
  pending_conflict: {
    request_id: "r",
    source_path: "/card/photo.jpg",
    destination_path: "/nas/photo.jpg",
    source_hash: "123",
    destination_hash: "456",
  },
};

describe("destination job dialogs", () => {
  const previousJobs = store.transfers;
  const previousDialogs = store.dialogs;
  let element: HTMLElement;
  afterEach(() => {
    element?.remove();
    store.transfers = previousJobs;
    store.dialogs = previousDialogs;
    vi.restoreAllMocks();
  });

  it("offers per-file decisions with explicit queue-only apply-all", async () => {
    store.transfers = [job];
    const resolve = vi.spyOn(store, "resolveTransferConflict").mockResolvedValue(true);
    const dialog = new OmbTransferConflictDialog();
    dialog.request = { type: "transfer-conflict", jobId: "j", requestId: "r" };
    element = dialog;
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
    expect(dialog.textContent).toContain("/card/photo.jpg");
    expect(dialog.textContent).toContain("/nas/photo.jpg");
    expect(dialog.textContent).toContain("without claiming matching hashes");
    expect(dialog.textContent).toContain("This choice will not affect future runs");
    const checkbox = dialog.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    expect(checkbox.checked).toBe(false);
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event("change"));
    [...dialog.querySelectorAll("button")]
      .find((button) => button.textContent?.trim() === "Replace")!
      .click();
    await Promise.resolve();
    expect(resolve).toHaveBeenCalledWith("j", "r", "replace", true);
  });

  it("cancels explicitly and prevents Escape from abandoning a blocked worker", async () => {
    store.transfers = [job];
    const cancel = vi.spyOn(store, "cancelTransfer").mockResolvedValue(true);
    const dialog = new OmbTransferConflictDialog();
    dialog.request = { type: "transfer-conflict", jobId: "j", requestId: "r" };
    element = dialog;
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
    const escape = new Event("cancel", { cancelable: true });
    dialog.querySelector("dialog")!.dispatchEvent(escape);
    expect(escape.defaultPrevented).toBe(true);
    [...dialog.querySelectorAll("button")]
      .find((button) => button.textContent?.includes("Cancel transfer"))!
      .click();
    expect(cancel).toHaveBeenCalledWith("j");
  });

  it("shows partial check results, paths, and explicit errors", async () => {
    const dialog = new OmbDestinationCheckResultsDialog();
    dialog.request = {
      type: "destination-check-results",
      job: {
        ...job,
        kind: "check",
        state: "cancelled",
        pending_conflict: null,
        errors: ["Disk disconnected"],
        check_results: {
          matched: 1,
          missing: 1,
          conflicts: 1,
          errors: 1,
          verified: 0,
          untracked: 0,
          items: [
            { source_path: "a.jpg", destination_path: "/nas/a.jpg", outcome: "matched", error: null },
            {
              source_path: "b.jpg",
              destination_path: "/nas/b.jpg",
              outcome: "error",
              error: "Permission denied",
            },
          ],
        },
      },
    };
    element = dialog;
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
    expect(dialog.textContent).toContain("Destination check cancelled");
    expect(dialog.textContent).toContain("partial results");
    expect(dialog.textContent).toContain("1 matching");
    expect(dialog.textContent).toContain("Permission denied");
    expect(dialog.textContent).toContain("Disk disconnected");
    expect(dialog.querySelector("li")?.textContent).toContain("/nas/b.jpg");
  });

  it("starts each new per-file prompt with apply-all unchecked", async () => {
    store.transfers = [job];
    store.dialogs = [{ type: "transfer-conflict", jobId: "j", requestId: "r" }];
    const host = new OmbDialogHost();
    element = host;
    document.body.append(host);
    await host.updateComplete;
    await host.querySelector<OmbTransferConflictDialog>("omb-transfer-conflict-dialog")!.updateComplete;
    await host.querySelector("omb-modal")!.updateComplete;
    const checkbox = host.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event("change"));
    store.transfers = [{ ...job, pending_conflict: { ...job.pending_conflict!, request_id: "next" } }];
    store.dialogs = [{ type: "transfer-conflict", jobId: "j", requestId: "next" }];
    store.dispatchEvent(new Event("change"));
    await host.updateComplete;
    await host.querySelector<OmbTransferConflictDialog>("omb-transfer-conflict-dialog")!.updateComplete;
    await host.querySelector("omb-modal")!.updateComplete;
    expect(host.querySelector<HTMLInputElement>('input[type="checkbox"]')!.checked).toBe(false);
  });
});
