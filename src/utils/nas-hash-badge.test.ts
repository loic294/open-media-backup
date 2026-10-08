import { describe, expect, it } from "vitest";
import type { HashServer } from "../api/types";
import { nasHashBadge } from "./nas-hash-badge";

const mapping = { enabled: true, server_id: "nas", root: "photos" };
const server: HashServer = {
  id: "nas",
  name: "Studio NAS",
  address: "nas:47822",
  last_seen: 123,
  last_error: null,
};

describe("NAS hash badge state", () => {
  it("reports the last successful connection without promising remote verification", () => {
    expect(nasHashBadge(mapping, [server])).toEqual({
      available: true,
      tooltip:
        "Connected to Studio NAS at the last connection check; NAS-side hash checks are available. Checks fall back to local re-reads if remote hashing fails.",
    });
  });

  it.each([
    [[], "hash server is not available on this computer"],
    [[{ ...server, last_error: "Connection refused" }], "Connection refused"],
    [[{ ...server, last_seen: null }], "no successful connection recorded"],
  ] as const)("reports unavailable runtime state for %j", (servers, reason) => {
    const badge = nasHashBadge(mapping, [...servers]);
    expect(badge?.available).toBe(false);
    expect(badge?.tooltip).toContain("Not connected");
    expect(badge?.tooltip).toContain(reason);
    expect(badge?.tooltip).toContain("local re-reads");
  });

  it("does not show enabled hashing for disabled or absent mappings", () => {
    expect(nasHashBadge({ ...mapping, enabled: false }, [server])).toBeNull();
    expect(nasHashBadge(null, [server])).toBeNull();
    expect(nasHashBadge(undefined, [server])).toBeNull();
  });
});
