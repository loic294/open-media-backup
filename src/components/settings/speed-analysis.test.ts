import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AnalysisJob, AnalysisSummary, TransferJob } from "../../api/types";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { emptyAnalysisMetrics, summarizeAnalysis } from "../../utils/speed-analysis";
import { OmbSpeedAnalysis } from "./speed-analysis";

function record(id = "one"): AnalysisJob {
  return {
    id,
    kind: "transfer",
    state: "done",
    phase: "finished",
    error_count: 0,
    created_at: Date.now() - 5000,
    updated_at: Date.now(),
    finished_at: Date.now(),
    context: {
      pair_id: "pair",
      space_id: "travel",
      space_name: "Travel",
      source_id: "s1",
      destination_id: "d1",
      source_device_id: "card1",
      destination_device_id: "ssd",
      source_name: "Card",
      destination_name: "Editing SSD",
      source_device_name: "Card",
      destination_device_name: "SSD",
      hash_algo: "blake3",
      verify_mode: "reread",
    },
    metrics: {
      ...emptyAnalysisMetrics(),
      copy_bytes: 1_000_000_000,
      committed_bytes: 1_000_000_000,
      copy_secs: 20,
      source_check_secs: 3,
      destination_check_secs: 7,
      transferred_files: 10,
    },
  };
}
function live(record: AnalysisJob): TransferJob {
  return {
    id: record.id,
    analysis: record,
    flow_id: "f1",
    label: "Card → Editing SSD",
    state: "running",
    kind: "transfer",
    files_done: 1,
    files_total: 10,
    bytes_done: 100_000_000,
    bytes_total: 1_000_000_000,
    current_file: null,
    speed_bps: 50_000_000,
    bytes_per_sec: 50_000_000,
    eta_secs: 18,
    errors: [],
  };
}
const filter = { space_id: null, since: null, pair_id: null };

describe("Speed Analysis settings", () => {
  const previous = { snapshot: store.snapshot, transfers: store.transfers };
  let element: OmbSpeedAnalysis;
  let jobs: AnalysisJob[];
  let summary: AnalysisSummary;

  beforeEach(() => {
    store.snapshot = demoSnapshot();
    store.transfers = [];
    jobs = [record()];
    summary = summarizeAnalysis(jobs, filter);
    vi.spyOn(store.backend, "getSpeedAnalysis").mockResolvedValue(summary);
    vi.spyOn(store.backend, "listSpeedAnalysisJobs").mockImplementation(async (req) => ({
      jobs: jobs.slice(req.offset, req.offset + req.limit),
      total: jobs.length,
    }));
    vi.spyOn(store.backend, "listTransfers").mockResolvedValue([]);
  });

  afterEach(() => {
    element?.remove();
    store.snapshot = previous.snapshot;
    store.transfers = previous.transfers;
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  async function mount() {
    element = new OmbSpeedAnalysis();
    document.body.append(element);
    await vi.waitFor(() => expect(element.textContent).not.toContain("Loading speed history"));
    await element.updateComplete;
    return element;
  }
  function select(label: string, value: string) {
    const select = element.querySelector<HTMLSelectElement>(`select[aria-label="${label}"]`)!;
    select.value = value;
    select.dispatchEvent(new Event("change"));
  }

  it("shows historical weighted speeds, phase breakdowns, job details and terminology", async () => {
    await mount();
    await vi.waitFor(() =>
      expect(element.querySelector('button[aria-controls="job-detail-one"]')).not.toBeNull(),
    );
    expect(element.textContent).toContain("50.0 MB/s");
    expect(element.textContent).toContain("Local checks (source)");
    expect(element.textContent).toContain("Destination re-reads");
    expect(element.textContent).toContain("NAS-side hash checks");
    expect(element.textContent).toContain("network re-read traffic");
    expect(element.textContent).toContain("not counted twice");
    const detail = [...element.querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent?.trim() === "Details",
    )!;
    detail.click();
    await element.updateComplete;
    expect(detail.getAttribute("aria-expanded")).toBe("true");
    expect(element.textContent).toContain("Verification: reread");
    expect(element.textContent?.replace(/\s+/g, " ")).toContain("Errors: 0");
    expect(element.querySelector("#job-detail-one")?.closest("table")).toBeNull();
  });

  it("shows live metrics separately and does not refetch history on every progress event", async () => {
    const current = record("live");
    current.state = "running";
    current.phase = "source_check";
    current.finished_at = null;
    current.metrics.source_check_bytes = 600_000_000;
    store.transfers = [live(current)];
    await mount();
    expect(element.querySelector("[data-live-job=live]")?.textContent).toContain("200 MB/s");
    expect(element.textContent).toContain("Phase average so far");
    expect(element.textContent).toContain("1 transfer");
    const requests = vi.mocked(store.backend.getSpeedAnalysis).mock.calls.length;
    store.dispatchEvent(new Event("change"));
    await element.updateComplete;
    expect(store.backend.getSpeedAnalysis).toHaveBeenCalledTimes(requests);
    store.transfers = [{ ...store.transfers[0], state: "done" }];
    store.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(store.backend.getSpeedAnalysis).toHaveBeenCalledTimes(requests + 1));
    expect(element.querySelector("[data-live-job=live]")).toBeNull();
  });

  it("reports query errors and retries instead of displaying zero statistics", async () => {
    vi.mocked(store.backend.getSpeedAnalysis).mockRejectedValueOnce(new Error("database unavailable"));
    await mount();
    expect(element.querySelector("[role=alert]")?.textContent).toContain("database unavailable");
    expect(element.querySelector(".stat-value")).toBeNull();
    element.querySelector<HTMLButtonElement>("[role=alert] button")!.click();
    await vi.waitFor(() => expect(element.textContent).toContain("50.0 MB/s"));
  });

  it("paginates job history, with All jobs changing page size", async () => {
    jobs = Array.from({ length: 7 }, (_, i) => record(`job-${i}`));
    await mount();
    await vi.waitFor(() => expect(element.textContent).toContain("1–5 of 7 jobs"));
    [...element.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent?.trim() === "Next")!
      .click();
    await vi.waitFor(() => expect(element.textContent).toContain("6–7 of 7 jobs"));
    expect(store.backend.listSpeedAnalysisJobs).toHaveBeenLastCalledWith(
      expect.objectContaining({ pair_id: "pair", offset: 5, limit: 5 }),
    );
    [...element.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent?.trim() === "All jobs")!
      .click();
    await vi.waitFor(() => expect(element.textContent).toContain("1–7 of 7 jobs"));
  });

  it("ignores stale filter responses and unsubscribes on disconnect", async () => {
    let resolveOld!: (value: AnalysisSummary) => void;
    vi.mocked(store.backend.getSpeedAnalysis).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOld = resolve;
        }),
    );
    element = new OmbSpeedAnalysis();
    document.body.append(element);
    await element.updateComplete;
    select("Analysis date range", "all");
    await vi.waitFor(() => expect(element.textContent).toContain("50.0 MB/s"));
    resolveOld({ pairs: [], totals: summarizeAnalysis([], filter).totals });
    await new Promise((resolve) => setTimeout(resolve, 0));
    await element.updateComplete;
    expect(element.textContent).toContain("50.0 MB/s");
    expect(store.backend.getSpeedAnalysis).toHaveBeenLastCalledWith(filter);
    element.remove();
    const calls = vi.mocked(store.backend.getSpeedAnalysis).mock.calls.length;
    store.transfers = [{ ...live(record()), state: "done" }];
    store.dispatchEvent(new Event("change"));
    expect(store.backend.getSpeedAnalysis).toHaveBeenCalledTimes(calls);
  });

  it("shows no history and check-only states without fabricated copy speeds", async () => {
    vi.mocked(store.backend.getSpeedAnalysis).mockResolvedValue({
      pairs: [],
      totals: summarizeAnalysis([], filter).totals,
    });
    await mount();
    expect(element.textContent).toContain("No recorded jobs for these filters");
    select("Analysis date range", "all");
    await vi.waitFor(() => expect(element.textContent).toContain("No speed history yet"));
    const check = record();
    check.kind = "check";
    check.metrics = { ...emptyAnalysisMetrics(), source_check_secs: 5, destination_check_secs: 8 };
    vi.mocked(store.backend.getSpeedAnalysis).mockResolvedValue(summarizeAnalysis([check], filter));
    select("Analysis date range", "7");
    await vi.waitFor(() => expect(element.textContent).toContain("0 transfers · 1 check"));
    expect(element.textContent).toContain("Not applicable");
  });

  it("keeps deleted spaces selectable when a filtered query has no records", async () => {
    jobs[0].context.space_id = "deleted-space";
    jobs[0].context.space_name = "Archived space";
    vi.mocked(store.backend.getSpeedAnalysis).mockResolvedValue(summarizeAnalysis(jobs, filter));
    await mount();
    vi.mocked(store.backend.getSpeedAnalysis).mockResolvedValue(summarizeAnalysis([], filter));
    select("Analysis space", "deleted-space");
    await vi.waitFor(() => expect(element.textContent).toContain("No recorded jobs for these filters"));
    expect(element.querySelector<HTMLSelectElement>('[aria-label="Analysis space"]')?.value).toBe(
      "deleted-space",
    );
    expect(element.textContent).toContain("Archived space");
    expect(store.backend.getSpeedAnalysis).toHaveBeenLastCalledWith(
      expect.objectContaining({ space_id: "deleted-space" }),
    );
  });

  it("ignores an old pair's delayed job history after selecting another pair", async () => {
    const other = record("other");
    other.context = { ...other.context, pair_id: "other-pair", destination_name: "NAS" };
    vi.mocked(store.backend.getSpeedAnalysis).mockResolvedValue(summarizeAnalysis([jobs[0], other], filter));
    let resolveOld!: (value: { jobs: AnalysisJob[]; total: number }) => void;
    vi.mocked(store.backend.listSpeedAnalysisJobs)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveOld = resolve;
          }),
      )
      .mockResolvedValue({ jobs: [other], total: 1 });
    await mount();
    select("Analysis pair", "other-pair");
    await vi.waitFor(() =>
      expect(element.querySelector('button[aria-controls="job-detail-other"]')).not.toBeNull(),
    );
    resolveOld({ jobs: [jobs[0]], total: 1 });
    await new Promise((resolve) => setTimeout(resolve, 0));
    await element.updateComplete;
    expect(element.querySelector('button[aria-controls="job-detail-one"]')).toBeNull();
    expect(element.querySelector('button[aria-controls="job-detail-other"]')).not.toBeNull();
  });

  it("polls active snapshots for growing pause durations and reports refresh failures", async () => {
    vi.useFakeTimers();
    const current = record("paused-live");
    current.state = "paused";
    current.phase = "paused";
    current.finished_at = null;
    const running = { ...live(current), state: "paused" as const };
    store.transfers = [running];
    vi.mocked(store.backend.listTransfers).mockResolvedValue([
      {
        ...running,
        analysis: { ...current, metrics: { ...current.metrics, paused_secs: 5 } },
      },
    ]);
    await mount();
    await vi.advanceTimersByTimeAsync(1000);
    await element.updateComplete;
    expect(element.querySelector("[data-live-job=paused-live]")?.textContent).toContain("Paused: 5s");
    vi.mocked(store.backend.listTransfers).mockRejectedValue(new Error("live unavailable"));
    await vi.advanceTimersByTimeAsync(1000);
    await element.updateComplete;
    expect(element.querySelector("[role=alert]")?.textContent).toContain("live unavailable");
  });
});
