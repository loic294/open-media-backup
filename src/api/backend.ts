import type {
  DestinationCheckScope,
  AnalysisFilter,
  AnalysisJobPage,
  AnalysisJobsRequest,
  AnalysisSummary,
  AppSettings,
  ConflictDecision,
  Destination,
  Device,
  DeviceMapping,
  EntityKind,
  FileCategory,
  FileRule,
  FileDirectory,
  FilePage,
  Flow,
  HashServer,
  HashRoot,
  HashBrowse,
  HashMappingTest,
  MediaMetadata,
  OpenAppImportResult,
  Project,
  ProjectStatus,
  RevealKind,
  Snapshot,
  Source,
  SourceSafeCopyDetails,
  Space,
  SyncStatus,
  TransferJob,
  UpdateInfo,
  UpdateProgress,
  Volume,
  WipeMethod,
  WipePlan,
  WorkspaceContext,
  WorkspaceFilesRequest,
  WorkspaceStatus,
} from "./types";

export type EntityByKind = {
  space: Space;
  project: Project;
  device: Device;
  device_mapping: DeviceMapping;
  computer: Snapshot["computer"];
  source: Source;
  destination: Destination;
  flow: Flow;
};

export interface BackendEvents {
  "snapshot-changed": void;
  "status-changed": void;
  transfers: TransferJob[];
  "transfer-power-warning": string;
  "sync-status": SyncStatus;
  "volumes-changed": Volume[];
  "update://progress": UpdateProgress;
}

export type Unlisten = () => void;

/** Everything the UI can ask the Rust core. Implemented by Tauri and by an in-memory mock. */
export interface Backend {
  getSnapshot(): Promise<Snapshot>;
  saveEntity<K extends EntityKind>(kind: K, entity: EntityByKind[K]): Promise<void>;
  deleteEntity(kind: EntityKind, id: string): Promise<void>;
  saveSettings(settings: AppSettings): Promise<void>;

  getProjectStatus(projectId: string): Promise<ProjectStatus>;
  getWorkspaceStatus(context: WorkspaceContext): Promise<WorkspaceStatus>;
  getSourceSafeCopyDetails(context: WorkspaceContext, sourceId: string): Promise<SourceSafeCopyDetails>;
  saveDeviceSafeCopyRules(context: WorkspaceContext, deviceId: string, rules: FileRule[]): Promise<void>;
  listWorkspaceFiles(req: WorkspaceFilesRequest): Promise<FilePage>;
  listFiles(req: {
    projectId: string;
    flowId: string;
    category: FileCategory;
    offset: number;
    limit: number;
    filter?: string;
    directory?: FileDirectory;
  }): Promise<FilePage>;
  thumbnail(absPath: string): Promise<string | null>;
  getMediaMetadata(absPath: string): Promise<MediaMetadata>;
  openMedia(absPath: string): Promise<void>;
  revealInFileManager(kind: RevealKind, id: string): Promise<void>;
  connectDestinationNetworkDrive(destinationId: string): Promise<void>;
  openFlowInApp(projectId: string, flowId: string): Promise<OpenAppImportResult>;
  confirmAppImport(projectId: string, flowId: string, token: string): Promise<string>;
  openWorkspaceFlowInApp(context: WorkspaceContext, flowId: string): Promise<OpenAppImportResult>;
  confirmWorkspaceAppImport(context: WorkspaceContext, flowId: string, token: string): Promise<string>;
  checkForUpdate(): Promise<UpdateInfo | null>;
  installUpdate(): Promise<void>;

  runFlow(projectId: string, flowId: string): Promise<void>;
  runAll(projectId: string): Promise<void>;
  runWorkspaceFlow(context: WorkspaceContext, flowId: string): Promise<void>;
  runWorkspaceAll(context: WorkspaceContext): Promise<void>;
  runWorkspaceDestination(context: WorkspaceContext, destinationId: string): Promise<string[]>;
  checkWorkspaceDestination(
    context: WorkspaceContext,
    destinationId: string,
    scope?: DestinationCheckScope,
  ): Promise<string[]>;
  resolveTransferConflict(
    jobId: string,
    requestId: string,
    decision: ConflictDecision,
    applyToRemaining: boolean,
  ): Promise<void>;
  setTransferPaused(jobId: string, paused: boolean): Promise<void>;
  setAllPaused(paused: boolean): Promise<void>;
  cancelTransfer(jobId: string): Promise<void>;
  listTransfers(): Promise<TransferJob[]>;
  getSpeedAnalysis(req: AnalysisFilter): Promise<AnalysisSummary>;
  listSpeedAnalysisJobs(req: AnalysisJobsRequest): Promise<AnalysisJobPage>;

  listVolumes(): Promise<Volume[]>;
  registerDevice(mountPath: string, device: Device): Promise<void>;
  relinkDevice(deviceId: string, mountPath: string): Promise<void>;
  pickFolder(defaultPath?: string): Promise<string | null>;
  pickPreviewApp(os: string): Promise<string | null>;

  planWipe(sourceId: string): Promise<WipePlan>;
  wipe(sourceId: string, method: WipeMethod): Promise<void>;
  markSourceManuallyWiped(sourceId: string): Promise<void>;

  syncStatus(): Promise<SyncStatus>;
  addPeer(address: string, token: string): Promise<void>;
  removePeer(peerId: string): Promise<void>;
  syncNow(): Promise<void>;
  listHashServers(): Promise<HashServer[]>;
  addHashServer(address: string, token: string): Promise<void>;
  removeHashServer(id: string): Promise<void>;
  hashServerRoots(id: string): Promise<HashRoot[]>;
  hashServerBrowse(id: string, root: string, path: string): Promise<HashBrowse>;
  testRemoteHashMapping(destinationId: string): Promise<HashMappingTest>;

  on<E extends keyof BackendEvents>(
    event: E,
    handler: (payload: BackendEvents[E]) => void,
  ): Promise<Unlisten>;
}
