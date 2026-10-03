import type { DeviceKind } from "../../api/types";
import type { IconName } from "./icons";

export const DEVICE_ICON: Record<DeviceKind, IconName> = {
  sd_card: "card-sim",
  ssd: "hard-drive",
  hdd: "hard-drive",
  nas: "server",
  computer: "laptop",
  camera: "camera",
  drone: "drone",
  other: "usb",
};

export const DEVICE_KIND_LABEL: Record<DeviceKind, string> = {
  sd_card: "Memory card",
  ssd: "External SSD",
  hdd: "Hard drive",
  nas: "NAS",
  computer: "Computer",
  camera: "Camera",
  drone: "Drone",
  other: "Other",
};

/** Tile colors per device kind (semantic daisyUI colors only). */
export const DEVICE_TONE: Record<DeviceKind, string> = {
  sd_card: "bg-primary/15 text-primary",
  ssd: "bg-success/15 text-success",
  hdd: "bg-base-300 text-base-content/60",
  nas: "bg-info/15 text-info",
  computer: "bg-secondary/15 text-secondary",
  camera: "bg-primary/15 text-primary",
  drone: "bg-accent/15 text-accent",
  other: "bg-base-300 text-base-content/70",
};
