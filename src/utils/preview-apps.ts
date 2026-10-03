import type { AppSettings, FileEntry, PreviewAppMediaType, PreviewAppSettings } from "../api/types";

const DEFAULT_PREVIEW_APPS: PreviewAppSettings = { photos: null, videos: null };

export function previewApps(settings: AppSettings): PreviewAppSettings {
  return { ...DEFAULT_PREVIEW_APPS, ...settings.preview_apps };
}

export function previewMediaType(file: Pick<FileEntry, "media">): PreviewAppMediaType {
  return file.media === "video" ? "videos" : "photos";
}

export function configuredPreviewApp(settings: AppSettings, file: Pick<FileEntry, "media">): string | null {
  return previewApps(settings)[previewMediaType(file)];
}

export function appDisplayName(app: string): string {
  const trimmed = app.trim().replace(/[\\/]+$/, "");
  const name = trimmed.split(/[\\/]/).pop() ?? trimmed;
  return name.replace(/\.(app|exe|bat|cmd|sh)$/i, "") || trimmed || "custom app";
}

export function systemPreviewAppName(os: string | undefined, type: PreviewAppMediaType): string | null {
  return os?.toLowerCase().includes("mac") ? (type === "photos" ? "Preview" : "QuickTime Player") : null;
}

export function previewAppChoiceLabel(
  settings: AppSettings,
  os: string | undefined,
  type: PreviewAppMediaType,
): string {
  const app = previewApps(settings)[type];
  if (app) return appDisplayName(app);
  const systemName = systemPreviewAppName(os, type);
  return systemName ? `System default (${systemName})` : "System default";
}

export function openInAppLabel(
  settings: AppSettings,
  os: string | undefined,
  file: Pick<FileEntry, "media">,
): string {
  const app = configuredPreviewApp(settings, file);
  if (app) return `Open in ${appDisplayName(app)}`;
  return `Open in ${systemPreviewAppName(os, previewMediaType(file)) ?? "default app"}`;
}
