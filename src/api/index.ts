import type { Backend } from "./backend";
import { createMockBackend } from "./mock/mock-backend";
import { tauriBackend } from "./tauri-backend";

/**
 * Demo mode is opt-in at build time (`vite --mode demo`, see .env.demo). Normal builds always talk to the
 * Rust core, and the sample backend is dropped from the bundle because this constant folds to false.
 */
export const DEMO = import.meta.env.VITE_OMB_DEMO === "1";

/** True when running inside the Tauri desktop shell. */
export const inDesktopShell = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const backend: Backend = DEMO
  ? createMockBackend({ seedRunningTransfer: true, tickMs: 2500 })
  : tauriBackend;

export type { Backend } from "./backend";
