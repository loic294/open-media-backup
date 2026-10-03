import type { DialogFilter, OpenDialogOptions } from "@tauri-apps/plugin-dialog";

const WINDOWS_APP_FILTER: DialogFilter = { name: "Applications", extensions: ["exe", "lnk"] };
const MACOS_APP_FILTER: DialogFilter = { name: "Applications", extensions: ["app"] };

export function appPickerOptions(os: string): OpenDialogOptions {
  const normalized = os.toLowerCase();
  if (normalized.includes("mac")) {
    return {
      defaultPath: "/Applications",
      canCreateDirectories: false,
      filters: [MACOS_APP_FILTER],
    };
  }
  if (normalized.includes("windows")) {
    return { filters: [WINDOWS_APP_FILTER] };
  }
  return {};
}
