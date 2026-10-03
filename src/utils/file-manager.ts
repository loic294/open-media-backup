export function fileManagerName(): "Finder" | "File Explorer" | "file manager" {
  const platform = navigator.platform.toLowerCase();
  const userAgent = navigator.userAgent.toLowerCase();
  if (platform.includes("mac") || userAgent.includes("mac os")) return "Finder";
  if (platform.includes("win") || userAgent.includes("windows")) return "File Explorer";
  return "file manager";
}
