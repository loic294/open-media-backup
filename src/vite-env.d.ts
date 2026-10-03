/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** "1" when built with `--mode demo`: use the simulated backend with sample data. */
  readonly VITE_OMB_DEMO?: string;
}
