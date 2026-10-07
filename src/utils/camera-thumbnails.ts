import type { AppSettings } from "../api/types";

/** Keep in sync with `thumbnails::camera::default_paths` in the Rust backend. */
export const DEFAULT_CAMERA_THUMBNAIL_PATHS = [
  "../THMBNL/{stem}T01.JPG",
  "../../MISC/THM/{folder}/{stem}.SCR",
  "../../MISC/THM/{folder}/{stem}.THM",
  "{stem}.THM",
];

export function cameraThumbnailPaths(settings: AppSettings): string[] {
  return settings.camera_thumbnail_paths ?? DEFAULT_CAMERA_THUMBNAIL_PATHS;
}
