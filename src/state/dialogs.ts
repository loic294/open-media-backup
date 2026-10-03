/** Every dialog the app can open, with its props. Rendered by <omb-dialog-host>. */
export type DialogRequest =
  | { type: "device-sync" }
  | { type: "app-settings" }
  | { type: "project"; projectId: string | null }
  | { type: "space-settings"; spaceId: string; focus?: "name" | "variables" }
  | { type: "source-settings"; sourceId: string | null; mountPath?: string }
  | { type: "destination-settings"; destinationId: string | null }
  | { type: "wipe-card"; sourceId: string }
  | { type: "preview"; flowId: string | null; category?: "to_transfer" | "transferred" | "ignored" | "error" }
  | { type: "confirm"; title: string; message: string; confirmLabel: string; danger?: boolean; onConfirm: () => void | Promise<void> };

export type DialogType = DialogRequest["type"];
