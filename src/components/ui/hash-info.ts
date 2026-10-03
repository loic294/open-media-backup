import type { HashAlgo } from "../../api/types";

export const HASH_LABEL: Record<HashAlgo, string> = { xxh64: "xxHash64", blake3: "BLAKE3" };

/** Shown in space settings so users can make an informed choice. */
export const HASH_INFO: Record<HashAlgo, { pros: string[]; cons: string[] }> = {
  xxh64: {
    pros: ["Fastest option; never slows down a card reader or NAS link", "Excellent at detecting accidental corruption"],
    cons: ["Not cryptographic: can't prove files weren't deliberately altered", "64-bit: rare collisions are theoretically possible on huge catalogs"],
  },
  blake3: {
    pros: ["Cryptographic 256-bit hash: tamper-evident and collision-proof", "Still very fast on modern CPUs (multi-threaded)"],
    cons: ["Uses more CPU than xxHash64, noticeable on older laptops", "Hashes are longer to store and compare"],
  },
};
