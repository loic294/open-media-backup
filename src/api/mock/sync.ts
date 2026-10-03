import type { SyncStatus } from "../types";

export function demoSync(): SyncStatus {
  const now = Date.now();
  return {
    listen_address: "100.92.3.11:47821",
    token: "omb-demo-7f3a-21c9",
    syncing: true,
    progress: 0.72,
    peers: [
      { id: "studio", name: "Studio PC", address: "100.92.7.21:47821", os: "windows", state: "syncing", progress: 0.64, last_synced: now - 60_000, latency_ms: 18, message: "Receiving 312 of 486 file records" },
      { id: "nas-agent", name: "Home NAS agent", address: "100.92.14.8:47821", os: "linux", state: "up_to_date", progress: 1, last_synced: now - 120_000, latency_ms: 42, message: null },
      { id: "imac", name: "iMac Office", address: "100.92.5.40:47821", os: "macos", state: "offline", progress: 0, last_synced: now - 3 * 86_400_000, latency_ms: null, message: "46 changes waiting" },
    ],
  };
}
