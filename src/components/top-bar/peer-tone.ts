import type { PeerState } from "../../api/types";

export const PEER_TONE: Record<PeerState, string> = {
  up_to_date: "bg-success text-success-content",
  syncing: "bg-info text-info-content",
  idle: "bg-neutral text-neutral-content",
  offline: "bg-base-300 text-base-content/60",
  error: "bg-error text-error-content",
};

export const PEER_LABEL: Record<PeerState, string> = {
  up_to_date: "Up to date",
  syncing: "Syncing",
  idle: "Idle",
  offline: "Offline",
  error: "Error",
};
