import type { HashServer, HashRoot, HashBrowse, HashMappingTest, Snapshot } from "../types";

export const DEMO_HASH_SERVER_ID = "de634a1b-14f7-4f0a-8f4f-6b6cfb2a4c83";
export const DEMO_HASH_ROOT_ID = "a".repeat(64);

/** Browser-only simulation; never sends requests or stores real credentials. */
export class MockHashServers {
  private servers: HashServer[] = [];

  list(): HashServer[] {
    return structuredClone(this.servers);
  }

  add(address: string, token: string): void {
    if (!address.trim() || !token.trim()) throw new Error("Address and pairing token are required");
    if (token !== "demo-token")
      throw new Error("Demo pairing token is demo-token (no network request is made)");
    this.servers = [
      {
        id: DEMO_HASH_SERVER_ID,
        name: "NAS hash server (demo)",
        address: address.trim(),
        last_seen: Date.now(),
        last_error: null,
      },
    ];
  }

  remove(id: string): void {
    this.servers = this.servers.filter((server) => server.id !== id);
  }

  roots(id: string): HashRoot[] {
    this.known(id);
    return [{ id: DEMO_HASH_ROOT_ID, name: "photos", path: "/data/photos" }];
  }

  browse(id: string, root: string, path: string): HashBrowse {
    this.known(id);
    const [base, ...parts] = root.split("/");
    const selected = [...parts, path].filter(Boolean).join("/");
    if (base !== DEMO_HASH_ROOT_ID || !["", "Archive"].includes(selected))
      throw new Error("Remote folder not found");
    return { path: selected, directories: selected ? [] : ["Archive"] };
  }

  test(snapshot: Snapshot, destinationId: string): HashMappingTest {
    const destination = snapshot.destinations.find((d) => d.id === destinationId);
    const mapping = destination?.remote_hash;
    if (!mapping) throw new Error("No remote hash mapping");
    this.browse(mapping.server_id, mapping.root, "");
    return {
      verified: false,
      message: "Demo only: remote folder selection is valid. No real files were hashed.",
    };
  }

  private known(id: string): void {
    if (!this.servers.some((server) => server.id === id))
      throw new Error("Hash server is not available on this computer");
  }
}
