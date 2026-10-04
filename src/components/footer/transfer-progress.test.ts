import { afterEach, describe, expect, it } from "vitest";
import type { TransferJob } from "../../api/types";
import { store } from "../../state";
import { OmbTransferProgress } from "./transfer-progress";

const job: TransferJob = {
  id: "job",
  flow_id: "f",
  label: "Camera to NAS",
  kind: "transfer",
  state: "awaiting_decision",
  files_done: 0,
  files_total: 1,
  bytes_done: 0,
  bytes_total: 100,
  current_file: "a.jpg",
  speed_bps: 100,
  bytes_per_sec: 100,
  eta_secs: 1,
  errors: [],
};

describe("job progress states", () => {
  const previous = store.transfers;
  let element: OmbTransferProgress;
  afterEach(() => {
    element?.remove();
    store.transfers = previous;
  });

  it("labels a blocked worker without displaying stale transfer speed", async () => {
    store.transfers = [job];
    element = new OmbTransferProgress();
    document.body.append(element);
    await element.updateComplete;
    expect(element.textContent).toContain("awaiting a decision");
    expect(element.textContent).toContain("Waiting for your decision");
    expect(element.textContent).not.toContain("100 B/s");
    expect(element.querySelector<HTMLButtonElement>('button[title="Pause"]')?.disabled).toBe(true);
    expect(element.querySelector('button[title="Cancel job"]')).not.toBeNull();
  });

  it("distinguishes hash checks from copying jobs", async () => {
    store.transfers = [{ ...job, kind: "check", state: "verifying" }];
    element = new OmbTransferProgress();
    document.body.append(element);
    await element.updateComplete;
    expect(element.textContent).toContain("destination check");
    expect(element.textContent).toContain("Checking hashes");
  });
});
