import type { Backend, EntityByKind } from "../api/backend";
import type {
  AppSettings,
  EntityKind,
  ProjectStatus,
  Snapshot,
  SyncStatus,
  TransferJob,
  UpdateInfo,
  Volume,
} from "../api/types";
import { connectDesktopMenu } from "../desktop/menu";
import { checkForUpdateOnLaunch } from "./updater";
import { debounce } from "../utils/debounce";
import { applyTheme } from "../utils/theme";
import type { DialogRequest } from "./dialogs";
import { activeProject, activeSpace } from "./selectors";
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

/** Single source of UI state. Emits "change" whenever anything changes. */
export class AppStore extends EventTarget {
  snapshot: Snapshot | null = null;
  status: ProjectStatus | null = null;
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

  constructor(readonly backend: Backend) {
    super();
  }

  get space() {
    return this.snapshot ? activeSpace(this.snapshot) : null;
  }

  get project() {
    return this.snapshot ? activeProject(this.snapshot, this.space?.id) : null;
  }

  async init(): Promise<void> {
    try {
      const b = this.backend;
      this.#unlisten = await Promise.all([
        b.on("snapshot-changed", () => void this.reloadSnapshot()),
        b.on("status-changed", () => this.refreshStatus()),
        b.on("transfers", (jobs) => this.#set({ transfers: jobs })),
        b.on("sync-status", (sync) => this.#set({ sync })),
        b.on("volumes-changed", (volumes) => this.#set({ volumes })),
      ]);
      const [snapshot, transfers, sync, volumes] = await Promise.all([
        b.getSnapshot(),
        b.listTransfers(),
        b.syncStatus(),
        b.listVolumes(),
      ]);
      this.#set({ snapshot, transfers, sync, volumes });
      applyTheme(snapshot.settings.theme);
      this.#unlisten.push(await connectDesktopMenu(this));
      await this.#loadStatus();
      void checkForUpdateOnLaunch(this);
    } catch (e) {
      console.error("init failed", e);
      this.#set({ error: String(e) });
    }
  }

  dispose(): void {
    this.#unlisten.forEach((u) => u());
  }

  async reloadSnapshot(): Promise<void> {
    this.#set({ snapshot: await this.backend.getSnapshot() });
  }

  refreshStatus = debounce(() => void this.#loadStatus(), 120);

  async #loadStatus(): Promise<void> {
    const id = this.project?.id;
    try {
      this.#set({ status: id ? await this.backend.getProjectStatus(id) : null });
    } catch (e) {
      console.error("status failed", e);
      this.toast("error", `Status failed: ${e}`);
    }
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

  async selectProject(projectId: string): Promise<void> {
    const space = this.space;
    if (!space || !this.snapshot) return;
    this.status = null;
    await this.saveSettings({
      active_project_by_space: { ...this.snapshot.settings.active_project_by_space, [space.id]: projectId },
    });
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
    if (this.snapshot) {
      const key = COLLECTION[kind];
      const items = (this.snapshot[key] as { id: string }[]).filter((e) => e.id !== id);
      this.#set({ snapshot: { ...this.snapshot, [key]: items } });
    }
    await this.#guard(() => this.backend.deleteEntity(kind, id));
    await this.reloadSnapshot();
    this.refreshStatus();
  }

  // ---- actions ----

  async runFlow(flowId: string): Promise<void> {
    const project = this.project;
    if (project) await this.#guard(() => this.backend.runFlow(project.id, flowId));
  }

  async runAll(): Promise<void> {
    const project = this.project;
    if (project) await this.#guard(() => this.backend.runAll(project.id));
  }

  async openFlowInApp(flowId: string): Promise<void> {
    const project = this.project;
    if (!project) return;
    const opened = await this.#guardResult(() => this.backend.openFlowInApp(project.id, flowId));
    if (!opened) return;
    const count = opened.files.length;
    this.open({
      type: "confirm",
      title: `Did ${opened.app_name} finish importing?`,
      message: `${count.toLocaleString("en-US")} ${count === 1 ? "file was" : "files were"} opened in ${opened.app_name}. Mark ${count === 1 ? "it" : "them"} as transferred once the import has finished.`,
      confirmLabel: "Mark as transferred",
      cancelLabel: "Not yet",
      onConfirm: async () => {
        const marked = await this.backend.confirmAppImport(project.id, flowId, opened.token);
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

  setAvailableUpdate(update: UpdateInfo | null): void {
    this.#set({ availableUpdate: update });
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
    Object.assign(this, patch);
    this.dispatchEvent(new Event("change"));
  }
}
