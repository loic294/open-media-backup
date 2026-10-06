import type { Backend, EntityByKind } from "../api/backend";
import type {
  AppSettings,
  ConflictDecision,
  EntityKind,
  Status,
  Snapshot,
  SyncStatus,
  TransferJob,
  UpdateInfo,
  Volume,
  WorkspaceContext,
  WorkspaceFilesRequest,
} from "../api/types";
import { connectDesktopMenu } from "../desktop/menu";
import { checkForUpdateOnLaunch } from "./updater";
import { debounce } from "../utils/debounce";
import { applyTheme } from "../utils/theme";
import type { DialogRequest } from "./dialogs";
import { activeSpace } from "./selectors";
import { validateProjectConfiguration } from "./projects";

export interface Toast {
  id: number;
  kind: "info" | "success" | "warning" | "error";
  message: string;
}

const COLLECTION = {
  space: "spaces",
  project: "projects",
  device: "devices",
  device_mapping: "mappings",
  computer: "computers",
  source: "sources",
  destination: "destinations",
  flow: "flows",
} as const satisfies Record<EntityKind, keyof Snapshot>;

let autoUpdatePrompted = false;

/** Single source of UI state. Emits "change" whenever anything changes. */
export class AppStore extends EventTarget {
  snapshot: Snapshot | null = null;
  status: Status | null = null;
  statusLoading = false;
  statusError: string | null = null;
  transfers: TransferJob[] = [];
  sync: SyncStatus | null = null;
  volumes: Volume[] = [];
  dialogs: DialogRequest[] = [];
  toasts: Toast[] = [];
  availableUpdate: UpdateInfo | null = null;
  error: string | null = null;
  /** Source card highlighted in the workspace (its flows stand out). */
  selectedSourceId: string | null = null;
  #toastId = 0;
  #unlisten: (() => void)[] = [];
  #statusSeq = 0;
  #snapshotSeq = 0;
  #statusContextKey = "";
  #disposed = false;
  #statusFlight: { signature: string; promise: Promise<void> } | null = null;
  #statusRefreshPending = false;
  #shownCheckResults = new Set<string>();
  #transfersSeq = 0;
  #activeJobsOnly = false;

  constructor(readonly backend: Backend) {
    super();
  }

  get space() {
    return this.snapshot ? activeSpace(this.snapshot) : null;
  }

  get context(): WorkspaceContext | null {
    const space = this.space;
    return space ? { spaceId: space.id, projectId: null } : null;
  }

  async init(): Promise<void> {
    this.#disposed = false;
    this.#activeJobsOnly = false;
    try {
      const b = this.backend;
      this.#unlisten = await Promise.all([
        b.on("snapshot-changed", () => void this.reloadSnapshot()),
        b.on("status-changed", () => this.refreshStatus()),
        b.on("transfers", (jobs) => this.#receiveTransfers(jobs)),
        b.on("transfer-power-warning", (message) => this.toast("warning", message, 10000)),
        b.on("sync-status", (sync) => this.#set({ sync })),
        b.on("volumes-changed", (volumes) => this.#set({ volumes })),
      ]);
      const transfersSeq = this.#transfersSeq;
      const [snapshot, transfers, sync, volumes] = await Promise.all([
        b.getSnapshot(),
        b.listTransfers(),
        b.syncStatus(),
        b.listVolumes(),
      ]);
      this.#set({ snapshot, sync, volumes });
      if (transfersSeq === this.#transfersSeq) this.#receiveTransfers(transfers);
      applyTheme(snapshot.settings.theme);
      this.#unlisten.push(await connectDesktopMenu(this));
      await this.#loadStatus();
      void checkForUpdateOnLaunch(this);
    } catch (e) {
      console.error("init failed", e);
      this.#set({ error: String(e) });
    }
  }

  async initActiveJobs(): Promise<void> {
    this.#disposed = false;
    this.#activeJobsOnly = true;
    try {
      const b = this.backend;
      this.#unlisten = await Promise.all([
        b.on("transfers", (jobs) => this.#receiveTransfers(jobs)),
        b.on("transfer-power-warning", (message) => this.toast("warning", message, 10000)),
      ]);
      const transfersSeq = this.#transfersSeq;
      const [snapshot, transfers] = await Promise.all([b.getSnapshot(), b.listTransfers()]);
      this.#set({ snapshot });
      if (transfersSeq === this.#transfersSeq) this.#receiveTransfers(transfers);
      applyTheme(snapshot.settings.theme);
    } catch (e) {
      console.error("init failed", e);
      this.#set({ error: String(e) });
    }
  }

  dispose(): void {
    this.#disposed = true;
    this.#statusSeq++;
    this.#snapshotSeq++;
    this.#unlisten.forEach((u) => u());
    this.#unlisten = [];
  }

  async reloadSnapshot(): Promise<void> {
    const seq = ++this.#snapshotSeq;
    try {
      const snapshot = await this.backend.getSnapshot();
      if (this.#disposed || seq !== this.#snapshotSeq) return;
      this.#set({ snapshot });
      this.refreshStatus();
    } catch (error) {
      if (this.#disposed || seq !== this.#snapshotSeq) return;
      console.error("snapshot failed", error);
      this.toast("error", `Could not refresh workspace: ${error}`);
    }
  }

  #scheduleStatus = debounce(() => !this.#disposed && void this.#loadStatus(), 120);

  refreshStatus = (): void => {
    if (this.#disposed) return;
    if (this.#statusFlight?.signature === this.#statusSignature()) {
      this.#statusRefreshPending = true;
      return;
    }
    this.#scheduleStatus();
  };

  retryStatus = (): Promise<void> => this.#loadStatus();

  #loadStatus(): Promise<void> {
    if (this.#disposed) return Promise.resolve();
    const signature = this.#statusSignature();
    if (this.#statusFlight?.signature === signature) return this.#statusFlight.promise;
    const flight = { signature, promise: Promise.resolve() };
    this.#statusRefreshPending = false;
    this.#statusFlight = flight;
    flight.promise = this.#fetchStatus().finally(() => {
      if (this.#statusFlight !== flight) return;
      this.#statusFlight = null;
      if (this.#statusRefreshPending && !this.#disposed) {
        this.#statusRefreshPending = false;
        this.#scheduleStatus();
      }
    });
    return flight.promise;
  }

  async #fetchStatus(): Promise<void> {
    const context = this.context;
    const key = JSON.stringify(context);
    const seq = ++this.#statusSeq;
    const current = () => !this.#disposed && seq === this.#statusSeq && key === JSON.stringify(this.context);
    this.#set({
      status: key === this.#statusContextKey ? this.status : null,
      statusLoading: !!context,
      statusError: null,
    });
    this.#statusContextKey = key;
    try {
      const status = context ? await this.backend.getWorkspaceStatus(context) : null;
      if (current()) this.#set({ status, statusLoading: false });
    } catch (e) {
      if (!current()) return;
      console.error("status failed", e);
      this.#set({ status: null, statusLoading: false, statusError: String(e) });
      this.toast("error", `Status failed: ${e}`);
    }
  }

  listWorkspaceFiles(req: WorkspaceFilesRequest) {
    return this.backend.listWorkspaceFiles(req);
  }

  // ---- selection & settings ----

  async saveSettings(patch: Partial<AppSettings>): Promise<void> {
    if (!this.snapshot) return;
    const settings = { ...this.snapshot.settings, ...patch };
    this.#set({ snapshot: { ...this.snapshot, settings } });
    if (patch.theme) applyTheme(settings.theme);
    await this.#guard(() => this.backend.saveSettings(settings));
  }

  async selectSpace(spaceId: string): Promise<void> {
    this.status = null;
    await this.saveSettings({ active_space_id: spaceId });
    await this.#loadStatus();
  }

  // ---- entities (optimistic) ----

  async save<K extends EntityKind>(kind: K, entity: EntityByKind[K]): Promise<boolean> {
    if (this.snapshot) {
      const key = COLLECTION[kind];
      const items = [...(this.snapshot[key] as { id: string }[])];
      const i = items.findIndex((e) => e.id === entity.id);
      if (i >= 0) items[i] = entity;
      else items.push(entity);
      const candidate = { ...this.snapshot, [key]: items };
      if (
        kind === "space" ||
        kind === "project" ||
        kind === "source" ||
        kind === "destination" ||
        kind === "flow"
      ) {
        const spaceId = kind === "space" ? entity.id : "space_id" in entity ? entity.space_id : null;
        try {
          if (spaceId) validateProjectConfiguration(candidate, spaceId);
        } catch (error) {
          console.error(error);
          this.toast("error", String(error));
          return false;
        }
      }
      this.#set({ snapshot: candidate });
    }
    const saved = await this.#guard(() => this.backend.saveEntity(kind, entity));
    this.refreshStatus();
    return saved;
  }

  async remove(kind: EntityKind, id: string): Promise<void> {
    if (kind === "device") {
      await this.removeDevice(id);
      return;
    }
    if (this.snapshot) {
      const key = COLLECTION[kind];
      const items = (this.snapshot[key] as { id: string }[]).filter((e) => e.id !== id);
      this.#set({ snapshot: { ...this.snapshot, [key]: items } });
    }
    await this.#guard(() => this.backend.deleteEntity(kind, id));
    await this.reloadSnapshot();
    this.refreshStatus();
  }

  async removeDevice(id: string): Promise<boolean> {
    const removed = await this.#guard(() => this.backend.deleteEntity("device", id));
    if (!removed) return false;
    await this.reloadSnapshot();
    await this.refreshVolumes();
    return true;
  }

  async refreshVolumes(): Promise<void> {
    await this.#guard(async () => {
      const volumes = await this.backend.listVolumes();
      this.#set({ volumes });
    });
  }

  // ---- actions ----

  #receiveTransfers(jobs: TransferJob[]): void {
    this.#transfersSeq++;
    if (this.#activeJobsOnly) {
      this.#set({ transfers: jobs });
      return;
    }
    const dialogs = this.dialogs.filter(
      (dialog) =>
        dialog.type !== "transfer-conflict" ||
        jobs.some(
          (job) =>
            job.id === dialog.jobId &&
            job.pending_conflict?.request_id === dialog.requestId &&
            !["done", "failed", "cancelled"].includes(job.state),
        ),
    );
    if (!dialogs.some((dialog) => dialog.type === "transfer-conflict")) {
      const waiting = jobs.find(
        (job) => job.pending_conflict && !["done", "failed", "cancelled"].includes(job.state),
      );
      if (waiting?.pending_conflict)
        dialogs.push({
          type: "transfer-conflict",
          jobId: waiting.id,
          requestId: waiting.pending_conflict.request_id,
        });
    }
    let completedCheck = false;
    for (const job of jobs) {
      if (
        job.kind !== "check" ||
        !["done", "failed", "cancelled"].includes(job.state) ||
        this.#shownCheckResults.has(job.id)
      )
        continue;
      this.#shownCheckResults.add(job.id);
      dialogs.push({ type: "destination-check-results", job });
      completedCheck = true;
    }
    const conflictIndex = dialogs.findIndex((dialog) => dialog.type === "transfer-conflict");
    if (conflictIndex >= 0) dialogs.push(...dialogs.splice(conflictIndex, 1));
    this.#set({ transfers: jobs, dialogs });
    if (completedCheck) this.refreshStatus();
  }

  async runDestination(destinationId: string): Promise<void> {
    const context = this.context;
    if (context) await this.#guard(() => this.backend.runWorkspaceDestination(context, destinationId));
  }

  async checkDestination(destinationId: string): Promise<void> {
    const context = this.context;
    if (context) await this.#guard(() => this.backend.checkWorkspaceDestination(context, destinationId));
  }

  async resolveTransferConflict(
    jobId: string,
    requestId: string,
    decision: ConflictDecision,
    applyToRemaining: boolean,
  ): Promise<boolean> {
    return this.#guard(async () => {
      await this.backend.resolveTransferConflict(jobId, requestId, decision, applyToRemaining);
      await this.#reloadTransfers();
    });
  }

  async cancelTransfer(jobId: string): Promise<boolean> {
    return this.#guard(async () => {
      await this.backend.cancelTransfer(jobId);
      await this.#reloadTransfers();
    });
  }

  async #reloadTransfers(): Promise<void> {
    const seq = this.#transfersSeq;
    const jobs = await this.backend.listTransfers();
    if (seq === this.#transfersSeq) this.#receiveTransfers(jobs);
  }

  async runFlow(flowId: string): Promise<void> {
    const context = this.context;
    if (context) await this.#guard(() => this.backend.runWorkspaceFlow(context, flowId));
  }

  async runAll(): Promise<void> {
    const context = this.context;
    if (context) await this.#guard(() => this.backend.runWorkspaceAll(context));
  }

  async openFlowInApp(flowId: string): Promise<void> {
    const context = this.context;
    if (!context) return;
    const opened = await this.#guardResult(() => this.backend.openWorkspaceFlowInApp(context, flowId));
    if (!opened) return;
    const count = opened.files.length;
    this.open({
      type: "confirm",
      title: `Did ${opened.app_name} finish importing?`,
      message: `${count.toLocaleString("en-US")} ${count === 1 ? "file was" : "files were"} opened in ${opened.app_name}. Mark ${count === 1 ? "it" : "them"} as transferred once the import has finished.`,
      confirmLabel: "Mark as transferred",
      cancelLabel: "Not yet",
      onConfirm: async () => {
        const marked = await this.backend.confirmWorkspaceAppImport(context, flowId, opened.token);
        this.toast(
          "success",
          `${marked.toLocaleString("en-US")} ${marked === 1 ? "file" : "files"} marked transferred`,
        );
        this.refreshStatus();
      },
    });
  }

  async revealInFileManager(kind: "source" | "destination", id: string): Promise<void> {
    await this.#guard(() => this.backend.revealInFileManager(kind, id));
  }

  async setPaused(jobId: string | null, paused: boolean): Promise<void> {
    await this.#guard(() =>
      jobId ? this.backend.setTransferPaused(jobId, paused) : this.backend.setAllPaused(paused),
    );
  }

  select(sourceId: string | null): void {
    this.#set({ selectedSourceId: this.selectedSourceId === sourceId ? null : sourceId });
  }

  // ---- dialogs & toasts ----

  open(dialog: DialogRequest): void {
    this.#set({ dialogs: [...this.dialogs, dialog] });
  }

  openProjectsDialog(): void {
    this.open({ type: "projects" });
  }

  close(dialog?: DialogRequest): void {
    this.#set({ dialogs: dialog ? this.dialogs.filter((d) => d !== dialog) : this.dialogs.slice(0, -1) });
  }

  openUpdateDialog(update = this.availableUpdate): void {
    if (!update || this.dialogs.some((d) => d.type === "update")) return;
    this.open({ type: "update", update });
  }

  setAvailableUpdate(update: UpdateInfo | null): void {
    this.#set({ availableUpdate: update });
    if (update && !autoUpdatePrompted) {
      autoUpdatePrompted = true;
      this.openUpdateDialog(update);
    }
  }

  toast(kind: Toast["kind"], message: string, ms = 4000): void {
    const toast = { id: ++this.#toastId, kind, message };
    this.#set({ toasts: [...this.toasts, toast] });
    setTimeout(() => this.#set({ toasts: this.toasts.filter((t) => t !== toast) }), ms);
  }

  async #guard(fn: () => Promise<unknown>): Promise<boolean> {
    try {
      await fn();
      return true;
    } catch (e) {
      console.error(e);
      this.toast("error", String(e));
      return false;
    }
  }

  async #guardResult<T>(fn: () => Promise<T>): Promise<T | null> {
    try {
      return await fn();
    } catch (e) {
      console.error(e);
      this.toast("error", String(e));
      return null;
    }
  }

  #set(patch: Partial<AppStore>): void {
    const previousContext = JSON.stringify(this.context);
    const previousStatusSignature = "snapshot" in patch ? this.#statusSignature() : "";
    Object.assign(this, patch);
    if ("snapshot" in patch) {
      this.#snapshotSeq++;
      if (previousStatusSignature !== this.#statusSignature()) {
        this.#statusSeq++;
        if (previousContext !== JSON.stringify(this.context)) this.status = null;
        this.statusError = null;
        this.statusLoading = !!this.context;
      }
    }
    this.dispatchEvent(new Event("change"));
  }

  #statusSignature(): string {
    return JSON.stringify({
      context: this.context,
      space: this.space,
      projects: this.snapshot?.projects,
      sources: this.snapshot?.sources,
      destinations: this.snapshot?.destinations,
      flows: this.snapshot?.flows,
      devices: this.snapshot?.devices,
      mappings: this.snapshot?.mappings,
    });
  }
}
