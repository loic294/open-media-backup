import type { Destination, Device, Source } from "../api/types";
import { appDisplayName } from "./preview-apps";

export function sourceTaskName(source: Source, device?: Device): string {
  return source.task_name?.trim() || device?.name || "Unknown device";
}

export function sourceBackupName(source: Source, device?: Device): string {
  return source.backup_name?.trim() || device?.name || "Unknown device";
}

/** Mirrors plan::sanitize_segment; task labels never pass through this helper. */
export function sanitizeBackupName(name: string): string {
  return name
    .replace(/[/\\:*?"<>|\p{Cc}]/gu, "_")
    .trim()
    .replace(/^\.+|\.+$/g, "");
}

export function destinationTaskName(
  destination: Destination,
  device?: Device,
  localAppPath?: string | null,
): string {
  return (
    destination.task_name?.trim() ||
    ((destination.kind ?? "folder") === "app"
      ? destination.app_name?.trim() || (localAppPath ? appDisplayName(localAppPath) : "Application")
      : device?.name || "Unknown device")
  );
}
