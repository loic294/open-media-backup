import type { HashAlgo, VerifyMode } from "../../api/types";
import { HASH_LABEL } from "../ui/hash-info";

export function verificationModeLabel(mode: VerifyMode): string {
  return mode === "reread" ? "re-read verification on" : "verification on";
}

export function hashVerificationSummary(hashAlgo: HashAlgo, verifyMode: VerifyMode): string {
  return `${HASH_LABEL[hashAlgo]} ${verificationModeLabel(verifyMode)}`;
}

export function hashVerificationTooltip(hashAlgo: HashAlgo, verifyMode: VerifyMode, failed: number): string {
  return [
    `Algorithm: ${HASH_LABEL[hashAlgo]}`,
    `Status: ${verificationModeLabel(verifyMode)}`,
    failed ? `Mismatches: ${failed.toLocaleString("en-US")} this run` : "Mismatches: 0 this run",
  ].join("\n");
}
