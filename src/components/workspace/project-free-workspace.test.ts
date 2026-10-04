import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { demoCounts, demoOffline, demoSnapshot } from "../../api/mock/data";
import { mockWorkspaceStatus } from "../../api/mock/status";
import type { FileEntry } from "../../api/types";
import { store } from "../../state";
import { OmbSourceCard } from "./source-card";
import { OmbDestinationCard } from "./destination-card";
import { OmbWorkspace } from "./omb-workspace";
import { OmbFooter } from "../footer/omb-footer";
import { OmbMediaBrowserDialog } from "../dialogs/media-browser-dialog";
import { OmbPreviewDialog } from "../dialogs/preview-dialog";
import { OmbWipeDialog } from "../dialogs/wipe-dialog";

async function until(condition: () => boolean) {
  for (let n = 0; n < 100; n++) {
    if (condition()) return;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  throw new Error("Workspace did not settle");
}

const file: FileEntry = {
  rel_path: "A.JPG",
  name: "A.JPG",
  size: 100,
  media: "image",
  abs_path: null,
  target_path: "Photos/A.JPG",
  category: "to_transfer",
  error: null,
  project_id: null,
};

describe("project-free workspace UI", () => {
  beforeEach(() => {
    store.snapshot = demoSnapshot();
    store.snapshot.projects = [];
    const destination = store.snapshot.destinations.find((d) => d.id === "d2")!;
    destination.path_template = "Photos";
    destination.rules = [];
    store.status = mockWorkspaceStatus(
      store.snapshot,
      store.context!,
      structuredClone(demoCounts),
      new Set(demoOffline),
    );
    store.statusLoading = false;
    store.statusError = null;
    store.dialogs = [];
    vi.spyOn(store, "toast").mockImplementation(() => {});
  });

  afterEach(() => {
    document.body.replaceChildren();
    store.snapshot = null;
    store.status = null;
    store.statusLoading = false;
    store.statusError = null;
    store.dialogs = [];
    vi.restoreAllMocks();
  });

  it("replaces source skeletons with counts and explains why wiping is disabled", async () => {
    const source = new OmbSourceCard();
    source.source = store.snapshot!.sources[0];
    document.body.append(source);
    await source.updateComplete;
    expect(source.querySelector(".skeleton")).toBeNull();
    expect(source.textContent).toContain("safe copies");
    expect(source.textContent).toContain("card-wiping safety requirements");
    expect(source.textContent).not.toContain("/null");
    expect([...source.querySelectorAll("button")].some((b) => b.textContent?.includes("Wipe card"))).toBe(
      false,
    );
  });

  it("shows actionable destination status and enables a project-free preview", async () => {
    const destination = new OmbDestinationCard();
    destination.destination = store.snapshot!.destinations.find((d) => d.id === "d2")!;
    const footer = new OmbFooter();
    document.body.append(destination, footer);
    await Promise.all([destination.updateComplete, footer.updateComplete]);
    expect(destination.querySelector(".skeleton")).toBeNull();
    expect(destination.textContent).toContain("to transfer");
    const run = [...destination.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Run")!;
    expect(run.disabled).toBe(false);
    const preview = [...footer.querySelectorAll("button")].find((b) => b.textContent?.includes("Preview"))!;
    expect(preview.disabled).toBe(false);
    preview.click();
    expect(store.dialogs.at(-1)).toMatchObject({ type: "preview" });
  });

  it("explains project-dependent path errors instead of offering a runnable transfer", async () => {
    const destination = new OmbDestinationCard();
    destination.destination = store.snapshot!.destinations.find((d) => d.id === "d1")!;
    document.body.append(destination);
    await destination.updateComplete;
    expect(destination.textContent).toContain("requires project values");
    expect(destination.querySelector(".skeleton")).toBeNull();
    const run = [...destination.querySelectorAll("button")].find((b) =>
      ["Run", "Retry"].includes(b.textContent?.trim() ?? ""),
    )!;
    expect(run.disabled).toBe(true);
  });

  it("renders a retryable status error without persistent skeletons", async () => {
    store.status = null;
    store.statusError = "Catalog unavailable";
    const retry = vi.spyOn(store, "retryStatus").mockResolvedValue();
    const workspace = new OmbWorkspace();
    document.body.append(workspace);
    await until(
      () => !!workspace.querySelector('[role="alert"]') && !!workspace.querySelector("omb-source-card"),
    );
    await (workspace.querySelector("omb-source-card") as OmbSourceCard).updateComplete;
    expect(workspace.querySelector(".skeleton")).toBeNull();
    expect(workspace.textContent).toContain("Catalog unavailable");
    [...workspace.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Retry")!.click();
    expect(retry).toHaveBeenCalledOnce();
  });

  it("loads source media and flow previews without a project", async () => {
    const list = vi.spyOn(store.backend, "listWorkspaceFiles").mockImplementation(async (req) => ({
      items: req.category === "to_transfer" ? [file] : [],
      total: req.category === "to_transfer" ? 1 : 0,
      total_bytes: req.category === "to_transfer" ? 100 : 0,
    }));
    const browser = new OmbMediaBrowserDialog();
    browser.request = { type: "media-browser", sourceId: "s1" };
    document.body.append(browser);
    await until(() => !!browser.querySelector('button[aria-label="Select A.JPG"]'));
    expect(browser.textContent).not.toContain("Select a project before browsing");
    expect(list.mock.calls.every(([req]) => req.context.projectId === null)).toBe(true);
    const preview = new OmbPreviewDialog();
    preview.request = { type: "preview", flowId: "f2" };
    document.body.append(preview);
    await until(() => preview.textContent?.includes("A.JPG") ?? false);
    expect(list).toHaveBeenCalledWith(
      expect.objectContaining({
        context: { spaceId: "travel", projectId: null },
        flowId: "f2",
      }),
    );
  });

  it("never leaves a project-free wipe dialog loading or offers a destructive action", async () => {
    const plan = vi.spyOn(store.backend, "planWipe");
    const dialog = new OmbWipeDialog();
    dialog.request = { type: "wipe-card", sourceId: "s1" };
    document.body.append(dialog);
    await dialog.updateComplete;
    await until(() => dialog.textContent?.includes("card-wiping safety requirements") ?? false);
    expect(dialog.querySelector(".loading")).toBeNull();
    expect(dialog.querySelector("button.btn-error")).toBeNull();
    expect(plan).not.toHaveBeenCalled();
  });
});
