import type { HashServer, RemoteHash } from "../api/types";

export function nasHashBadge(mapping: RemoteHash | null | undefined, servers: HashServer[]) {
  if (!mapping?.enabled) return null;
  const server = servers.find((candidate) => candidate.id === mapping.server_id);
  const fallback = "Checks fall back to local re-reads if remote hashing fails.";
  if (!server) {
    return {
      available: false,
      tooltip: `Not connected: hash server is not available on this computer. ${fallback}`,
    };
  }
  if (server.last_error) {
    return {
      available: false,
      tooltip: `Not connected to ${server.name}: ${server.last_error}. ${fallback}`,
    };
  }
  if (server.last_seen == null) {
    return {
      available: false,
      tooltip: `Not connected to ${server.name}: no successful connection recorded. ${fallback}`,
    };
  }
  return {
    available: true,
    tooltip: `Connected to ${server.name} at the last connection check; NAS-side hash checks are available. ${fallback}`,
  };
}
