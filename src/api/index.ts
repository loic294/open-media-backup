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

const demoUpdate =
  typeof window !== "undefined" && new URLSearchParams(window.location.search).get("ombUpdate") === "1";

export const backend: Backend = DEMO
  ? createMockBackend({ seedRunningTransfer: true, tickMs: 2500, demoUpdate })
  : tauriBackend;

export type { Backend } from "./backend";
