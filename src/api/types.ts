// Mirrors src-tauri/src/domain and src-tauri/src/app, plan, transfer, devices, sync, wipe. Keep in sync.

export type HashAlgo = "xxh64" | "blake3";
export type VerifyMode = "inline" | "reread";
export type DeviceRole = "original" | "temporary" | "final";
export type DeviceKind = "sd_card" | "ssd" | "hdd" | "nas" | "computer" | "camera" | "drone" | "other";
export type RuleAction = "include" | "exclude";
export type RuleSyntax = "glob" | "regex";
export type RuleExpr =
  | { op: "eq"; var: string; value: string }
  | { op: "ne"; var: string; value: string }
  | { op: "not"; item: RuleExpr }
  | { op: "and"; items: RuleExpr[] }
  | { op: "or"; items: RuleExpr[] };
export type EntityKind =
  "space" | "project" | "device" | "device_mapping" | "computer" | "source" | "destination" | "flow";
export type RevealKind = "source" | "destination";

export interface VariableDef {
  name: string;
  default_value: string;
  required: boolean;
}

export interface Space {
  id: string;
  name: string;
  icon: string;
  position: number;
  hash_algo: HashAlgo;
  verify_mode: VerifyMode;
  variables: VariableDef[];
  backup_marker_template: string;
  /** Missing in older snapshots means true. */
  allow_project_overlap?: boolean;
  /** 0 or missing means temporary destinations never count as safe copies. */
  temporary_copies_per_final?: number;
}

export type ProjectGranularity = "minute" | "day" | "year";
export type ProjectScope = { mode: "all" } | { mode: "selected"; project_ids: string[] } | { mode: "none" };

export interface Project {
  id: string;
  space_id: string;
  name: string;
  values: Record<string, string>;
  final_copies_required: number;
  archived: boolean;
  /** Inclusive embedded capture bounds in Unix milliseconds; absent on legacy projects. */
  start_time?: number | null;
  end_time?: number | null;
  /** UTC calendar buckets; the entire final bucket is included. */
  granularity?: ProjectGranularity;
  color?: string;
}

export interface Device {
  id: string;
  name: string;
  description: string;
  role: DeviceRole;
  kind: DeviceKind;
  hw_serial: string | null;
  volume_uuid: string | null;
  capacity_bytes: number | null;
}

export interface DeviceMapping {
  id: string;
  device_id: string;
  computer_id: string;
  root_path: string;
}

export interface Computer {
  id: string;
  name: string;
  os: string;
}

export interface PathFileRule {
  action: RuleAction;
  syntax: RuleSyntax;
  pattern: string;
}

export interface ConditionFileRule {
  kind: "condition";
  expr: RuleExpr;
  /** Path-rule fields are absent at runtime; optional here keeps legacy rule editors type-compatible. */
  action?: RuleAction;
  syntax?: RuleSyntax;
  pattern?: string;
}

export type FileRule = PathFileRule | ConditionFileRule;

export interface Source {
  id: string;
  space_id: string;
  device_id: string;
  /** Display-only card label; blank or missing uses the physical device name. */
  task_name?: string;
  /** Per-source backup name for source_name and subfolders; blank uses the device name. */
  backup_name?: string;
  path_template: string;
  offer_wipe: boolean;
  position: number;
  /** Missing in older snapshots means all projects. */
  project_scope?: ProjectScope;
}

export interface Destination {
  id: string;
  space_id: string;
  /** Display-only card label; blank or missing uses the device/application name. */
  task_name?: string;
  /** Missing in older snapshots means folder. */
  kind?: "folder" | "app";
  device_id: string;
  path_template: string;
  app_name?: string | null;
  subfolder_per_source: boolean;
  preserve_file_structure: boolean;
  counts_as_safe_copy: boolean;
  use_backup_marker: boolean;
  rules: FileRule[];
  position: number;
}

export interface Flow {
  id: string;
  space_id: string;
  source_id: string;
  destination_id: string;
}

export type ThemePreference = "system" | "light" | "dark";
export type PreviewAppMediaType = "photos" | "videos";

export interface PreviewAppSettings {
  photos: string | null;
  videos: string | null;
}

export interface AppSettings {
  theme: ThemePreference;
  auto_sync: boolean;
  auto_sync_minutes: number;
  sync_port: number;
  active_space_id: string | null;
  /** Per-device custom apps. Missing on older snapshots means system defaults. */
  preview_apps?: PreviewAppSettings;
  /** Missing in older snapshots means mounted devices appear first in device lists. */
  show_mounted_devices_first?: boolean;
  /** Local sleep prevention during active transfers. Missing on older snapshots means enabled. */
  keep_awake_during_transfers?: boolean;
  /** Local per-computer app paths for App destinations, keyed by destination id. */
  app_destinations?: Record<string, string>;
  /** Learned transfer throughput in bytes/sec, keyed by destination device id. `_global` is the fallback. */
  transfer_speeds?: Record<string, number>;
}

export interface Snapshot {
  computer: Computer;
  computers: Computer[];
  spaces: Space[];
  projects: Project[];
  devices: Device[];
  mappings: DeviceMapping[];
  sources: Source[];
  destinations: Destination[];
  flows: Flow[];
  settings: AppSettings;
}

export type FlowState = "done" | "pending" | "error" | "unavailable" | "empty";

export interface FlowStatus {
  flow_id: string;
  state: FlowState;
  transferred: number;
  to_transfer: number;
  ignored: number;
  failed: number;
  bytes_to_transfer: number;
  error: string | null;
  runnable?: boolean;
}

export interface SourceStatus {
  source_id: string;
  available: boolean;
  root_path: string | null;
  file_count: number;
  total_bytes: number;
  /** Shared across all sources on the same device within this workspace. */
  safe_copies: number;
  required_copies: number | null;
  wipe_eligible: boolean;
  blocking_reason: string | null;
}

export interface DestinationStatus {
  destination_id: string;
  available: boolean;
  root_path: string | null;
  free_bytes: number | null;
  transferred: number;
  to_transfer: number;
  ignored: number;
  failed: number;
  bytes_to_transfer: number;
  last_error: string | null;
}

export interface ProjectStatus {
  project_id: string;
  flows: FlowStatus[];
  sources: SourceStatus[];
  destinations: DestinationStatus[];
}

export interface WorkspaceContext {
  spaceId: string;
  projectId: string | null;
}

export interface WorkspaceStatus {
  context: WorkspaceContext;
  flows: FlowStatus[];
  sources: SourceStatus[];
  destinations: DestinationStatus[];
}

export type Status = ProjectStatus | WorkspaceStatus;

export interface WorkspaceFilesRequest {
  context: WorkspaceContext;
  flowId: string;
  category: FileCategory;
  offset: number;
  limit: number;
  filter?: string;
  directory?: FileDirectory;
}

export type FileCategory = "to_transfer" | "transferred" | "ignored" | "error";
export type MediaKind = "image" | "video" | "raw" | "other";

export type CaptureTimeSource =
  | "exif_original"
  | "exif_digitized"
  | "video_creation_time"
  | "video_original_date"
  | "sidecar_xml"
  | "mp4_header";

export interface CaptureTime {
  /** Embedded local wall time. Do not infer a timezone if utc_offset_seconds is null. */
  local_datetime: string;
  utc_offset_seconds: number | null;
  source: CaptureTimeSource;
}

export interface MediaMetadata {
  media_type: MediaKind;
  size_bytes: number;
  capture_time: CaptureTime | null;
  dimensions: { width: number; height: number } | null;
  camera: {
    make: string | null;
    model: string | null;
    lens_make: string | null;
    lens_model: string | null;
  };
  exposure: {
    shutter_seconds: number | null;
    aperture_f_number: number | null;
    iso: number | null;
    focal_length_mm: number | null;
    focal_length_35mm: number | null;
    compensation_ev: number | null;
  };
  orientation: number | null;
  video: {
    container: string | null;
    codec: string | null;
    pixel_format: string | null;
    duration_seconds: number | null;
    frame_rate: number | null;
    bit_rate_bps: number | null;
    rotation_degrees: number | null;
    audio: { codec: string | null; channels: number | null; sample_rate_hz: number | null }[];
  } | null;
}

export interface FileEntry {
  rel_path: string;
  name: string;
  size: number;
  media: MediaKind;
  abs_path: string | null;
  target_path: string | null;
  category: FileCategory;
  ignore_reason?: string | null;
  error: string | null;
  /** Embedded capture timestamp only (Unix milliseconds), never filesystem mtime. */
  capture_time?: number | null;
  /** Matching project for this route; overlapping projects produce separate route entries. */
  project_id?: string | null;
  /** Populated only when metadata extraction succeeded for the file. */
  metadata?: MediaMetadata | null;
}

export interface FileDirectory {
  kind: "destination" | "source";
  path: string;
}

export interface DirectorySummary extends FileDirectory {
  total: number;
  total_bytes: number;
  direct_files: number;
}

export interface FilePage {
  total: number;
  total_bytes: number;
  items: FileEntry[];
  directories?: DirectorySummary[];
}

export interface AppImportFile {
  rel_path: string;
  project_id: string | null;
}

export interface OpenAppImportResult {
  token: string;
  app_name: string;
  files: AppImportFile[];
}

export interface UpdateInfo {
  version: string;
  current_version: string;
  notes: string | null;
  date: string | null;
}

export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

export type TransferState =
  "queued" | "running" | "verifying" | "awaiting_decision" | "paused" | "done" | "failed" | "cancelled";

export type ConflictDecision = "skip" | "keep_both" | "replace";

export interface TransferConflict {
  request_id: string;
  source_path: string;
  destination_path: string;
  source_hash: string;
  destination_hash: string;
}

export interface DestinationCheckItem {
  source_path: string;
  destination_path: string;
  outcome: "matched" | "missing" | "conflict" | "error";
  error: string | null;
}

export interface DestinationCheckResults {
  matched: number;
  missing: number;
  conflicts: number;
  errors: number;
  /** Paths needing attention; matching files are counted without adding snapshot payload. */
  items: DestinationCheckItem[];
}

export interface TransferJob {
  id: string;
  flow_id: string;
  label: string;
  state: TransferState;
  files_done: number;
  files_total: number;
  bytes_done: number;
  bytes_total: number;
  current_file: string | null;
  /** Deprecated compatibility alias for bytes_per_sec, or 0 before speed is known. */
  speed_bps: number;
  bytes_per_sec: number | null;
  eta_secs: number | null;
  errors: string[];
  kind?: "transfer" | "check" | "wipe";
  pending_conflict?: TransferConflict | null;
  check_results?: DestinationCheckResults | null;
  analysis?: AnalysisJob | null;
}

export type AnalysisPhase =
  | "queued"
  | "other"
  | "copy"
  | "source_check"
  | "destination_check"
  | "paused"
  | "awaiting_decision"
  | "finished";

export interface AnalysisContext {
  pair_id: string;
  space_id: string;
  space_name: string;
  source_id: string;
  destination_id: string;
  source_device_id: string;
  destination_device_id: string;
  source_name: string;
  destination_name: string;
  source_device_name: string;
  destination_device_name: string;
  hash_algo: HashAlgo;
  verify_mode: VerifyMode;
}

export interface AnalysisMetrics {
  queued_secs: number;
  other_secs: number;
  copy_secs: number;
  source_check_secs: number;
  destination_check_secs: number;
  paused_secs: number;
  decision_secs: number;
  /** Physical writes, including retries; independent of UI progress budgets. */
  copy_bytes: number;
  committed_bytes: number;
  source_check_bytes: number;
  destination_check_bytes: number;
  transferred_files: number;
  adopted_files: number;
  skipped_files: number;
}

export interface AnalysisJob {
  id: string;
  context: AnalysisContext;
  kind: "transfer" | "check";
  state: TransferState | "interrupted";
  phase: AnalysisPhase;
  created_at: number;
  updated_at: number;
  finished_at: number | null;
  metrics: AnalysisMetrics;
  error_count: number;
}

export interface AnalysisTotals {
  metrics: AnalysisMetrics;
  completed_transfer_jobs: number;
  completed_check_jobs: number;
  failed_jobs: number;
  cancelled_jobs: number;
  interrupted_jobs: number;
  avg_copy_bps: number | null;
  effective_bps: number | null;
}

export interface AnalysisSummary {
  pairs: { context: AnalysisContext; totals: AnalysisTotals }[];
  totals: AnalysisTotals;
}

export interface AnalysisFilter {
  space_id: string | null;
  since: number | null;
  pair_id: string | null;
}

export interface AnalysisJobsRequest extends AnalysisFilter {
  offset: number;
  limit: number;
}

export interface AnalysisJobPage {
  jobs: AnalysisJob[];
  total: number;
}

export type VolumeMatch = "hw_serial" | "marker" | "volume_uuid" | "mapping";

export interface Volume {
  mount_path: string;
  name: string;
  volume_uuid: string | null;
  hw_serial: string | null;
  total_bytes: number | null;
  free_bytes: number | null;
  removable: boolean;
  device_id: string | null;
  matched_by: VolumeMatch | null;
}

export type WipeMethod = "delete_files" | "quick_format";

export interface WipePlan {
  source_id: string;
  files_total: number;
  ignored: number;
  copies: { device_id: string; device_name: string; verified: number; total: number }[];
  eligible: boolean;
  reason: string | null;
}

export type PeerState = "idle" | "syncing" | "up_to_date" | "offline" | "error";

export interface PeerStatus {
  id: string;
  name: string;
  address: string;
  os: string;
  state: PeerState;
  progress: number;
  last_synced: number | null;
  latency_ms: number | null;
  message: string | null;
}

export interface SyncStatus {
  listen_address: string;
  token: string;
  syncing: boolean;
  progress: number;
  peers: PeerStatus[];
}
