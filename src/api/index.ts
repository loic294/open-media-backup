import type { Backend } from "./backend";
import { createMockBackend } from "./mock/mock-backend";
import { tauriBackend } from "./tauri-backend";

/**
 * True inside our Tauri shell. TAURI_ENV_PLATFORM is only injected by the Tauri CLI, which avoids
 * misdetecting other Tauri-based hosts (embedded browsers). `?mock` forces the demo backend.
 */
export const isTauri = () =>
  typeof window !== "undefined" &&
  "__TAURI_INTERNALS__" in window &&
  !!import.meta.env.TAURI_ENV_PLATFORM &&
  !new URLSearchParams(location.search).has("mock");

/** The real Rust core inside Tauri; a simulated in-memory backend in a plain browser. */
export const backend: Backend = isTauri() ? tauriBackend : createMockBackend();

export type { Backend } from "./backend";
