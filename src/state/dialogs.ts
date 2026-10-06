/** Every dialog the app can open, with its props. Rendered by <omb-dialog-host>. */
export type DialogRequest =
  | { type: "device-sync" }
  | { type: "app-settings" }
  | { type: "device-settings"; deviceId: string | null }
  | {
      type: "update";
      update: import("../api/types").UpdateInfo;
    }
  | { type: "projects" }
  | {
      type: "project";
      projectId: string | null;
      start_time?: number;
      end_time?: number;
    }
  | { type: "space-settings"; spaceId: string; focus?: "name" | "variables" }
  | { type: "source-settings"; sourceId: string | null; mountPath?: string }
  | { type: "destination-settings"; destinationId: string | null }
  | { type: "wipe-card"; sourceId: string }
  | { type: "safe-copy"; sourceId: string }
  | { type: "preview"; flowId: string | null; category?: "to_transfer" | "transferred" | "ignored" | "error" }
  | { type: "media-browser"; sourceId: string }
  | { type: "transfer-conflict"; jobId: string; requestId: string }
  | { type: "destination-check-results"; job: import("../api/types").TransferJob }
  | {
      type: "destination-check";
      destinationId: string;
      context: import("../api/types").WorkspaceContext;
    }
  | {
      type: "confirm";
      title: string;
      message: string;
      confirmLabel: string;
      cancelLabel?: string;
      danger?: boolean;
      onConfirm: () => void | Promise<void>;
    };

export type DialogType = DialogRequest["type"];
