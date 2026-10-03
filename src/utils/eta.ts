export const DEFAULT_TRANSFER_SPEED_BPS = 120_000_000;
export const GLOBAL_TRANSFER_SPEED_KEY = "_global";

export type TransferSpeeds = Record<string, number | undefined> | null | undefined;
export type TransferSpeedSource = { bytes_per_sec?: number | null; speed_bps?: number | null };

export function liveTransferSpeed(job: TransferSpeedSource): number | null {
  return positiveSpeed(job.bytes_per_sec) ?? positiveSpeed(job.speed_bps) ?? null;
}

export function learnedTransferSpeed(
  speeds: TransferSpeeds,
  destinationDeviceId: string | null | undefined,
  fallback = DEFAULT_TRANSFER_SPEED_BPS,
): number {
  const deviceSpeed = destinationDeviceId ? speeds?.[destinationDeviceId] : undefined;
  const globalSpeed = speeds?.[GLOBAL_TRANSFER_SPEED_KEY];
  return positiveSpeed(deviceSpeed) ?? positiveSpeed(globalSpeed) ?? fallback;
}

export function estimateTransferSeconds(
  bytes: number,
  speeds: TransferSpeeds,
  destinationDeviceId: string | null | undefined,
  fallback = DEFAULT_TRANSFER_SPEED_BPS,
): number | null {
  if (bytes <= 0) return null;
  const speed = learnedTransferSpeed(speeds, destinationDeviceId, fallback);
  return speed > 0 ? bytes / speed : null;
}

function positiveSpeed(speed: number | null | undefined): number | undefined {
  return speed && speed > 0 ? speed : undefined;
}
