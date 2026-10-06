import { afterEach, describe, expect, it } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { formatProjectTime, OmbProjectDialog, parseProjectTime } from "./project-dialog";

describe("project dialog prefill", () => {
  const previousSnapshot = store.snapshot;
  let dialog: OmbProjectDialog | undefined;
  afterEach(() => {
    dialog?.remove();
    store.snapshot = previousSnapshot;
  });

  const mount = async (projectId: string | null) => {
    store.snapshot = demoSnapshot();
    dialog = new OmbProjectDialog();
    const bounds = {
      start_time: Date.parse("2026-06-15T18:27:34.567Z"),
      end_time: Date.parse("2026-06-16T19:28:45.678Z"),
    };
    dialog.request = {
      type: "project",
      projectId,
      ...bounds,
    };
    document.body.append(dialog);
    await dialog.updateComplete;
    await dialog.querySelector("omb-modal")!.updateComplete;
    return dialog;
  };

  it("populates the range for a new project created from selected media", async () => {
    const element = await mount(null);
    expect(
      element.querySelector<HTMLInputElement>('[aria-label="Inclusive capture range start"]')!.value,
    ).toBe("2026-06-15T18:27");
    expect(element.querySelector<HTMLInputElement>('[aria-label="Inclusive capture range end"]')!.value).toBe(
      "2026-06-16T19:28",
    );
  });

  it("does not replace an existing project's range with prefill", async () => {
    const snapshot = demoSnapshot();
    const projectId = snapshot.projects.find((project) => !project.archived)!.id;
    const element = await mount(projectId);
    const project = store.snapshot!.projects.find((p) => p.id === projectId)!;
    expect(
      element.querySelector<HTMLInputElement>('[aria-label="Inclusive capture range start"]')!.value,
    ).toBe(formatProjectTime(project.start_time, project.granularity ?? "minute"));
  });
});

describe("project dialog capture-time inputs", () => {
  it("formats and parses minute and day bounds in UTC", () => {
    const timestamp = Date.parse("2026-06-15T18:27:00Z");
    expect(formatProjectTime(timestamp, "minute")).toBe("2026-06-15T18:27");
    expect(parseProjectTime("2026-06-15T18:27", "minute")).toBe(timestamp);
    expect(formatProjectTime(timestamp, "day")).toBe("2026-06-15");
    expect(parseProjectTime("2026-06-15", "day")).toBe(Date.parse("2026-06-15T00:00:00Z"));
  });

  it("uses the start of the selected UTC year and supports empty bounds", () => {
    const yearStart = Date.parse("2026-01-01T00:00:00Z");
    expect(formatProjectTime(yearStart, "year")).toBe("2026");
    expect(parseProjectTime("2026", "year")).toBe(yearStart);
    expect(parseProjectTime("", "year")).toBeNull();
    expect(parseProjectTime("26", "year")).toBeNull();
  });
});
