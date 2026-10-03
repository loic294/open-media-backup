import type { FlowState } from "../../api/types";

/** CSS color per flow state, matching the legend. */
export const FLOW_COLOR: Record<FlowState, string> = {
  done: "var(--color-success)",
  pending: "var(--color-warning)",
  error: "var(--color-error)",
  unavailable: "color-mix(in oklab, var(--color-base-content) 35%, transparent)",
  empty: "color-mix(in oklab, var(--color-base-content) 25%, transparent)",
};

export const FLOW_LABEL: Record<FlowState, string> = {
  done: "Transferred",
  pending: "To transfer",
  error: "Issue",
  unavailable: "Unavailable",
  empty: "Nothing to copy",
};
