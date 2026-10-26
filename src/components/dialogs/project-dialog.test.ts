import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { nextProjectColor, PROJECT_COLORS } from "../../state/projects";
import { formatProjectTime, OmbProjectDialog, parseProjectTime } from "./project-dialog";

describe("project dialog prefill", () => {
  const previousSnapshot = store.snapshot;
  let dialog: OmbProjectDialog | undefined;
  afterEach(() => {
    dialog?.remove();
    store.snapshot = previousSnapshot;
    vi.restoreAllMocks();
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
    vi.spyOn(Math, "random").mockReturnValue(0.75);
    const element = await mount(null);
    const expectedColor = nextProjectColor(
      store.snapshot!.projects.filter((project) => project.space_id === store.space!.id),
      () => 0.75,
    );
    expect(element.querySelector('button[aria-pressed="true"]')?.getAttribute("aria-label")).toBe(
      `Choose project color ${expectedColor.toUpperCase()}`,
    );
    expect(
      element.querySelector<HTMLInputElement>('[aria-label="Inclusive capture range start"]')!.value,
    ).toBe(formatProjectTime(Date.parse("2026-06-15T18:27:34.567Z"), "minute"));
    expect(element.querySelector<HTMLInputElement>('[aria-label="Inclusive capture range end"]')!.value).toBe(
      formatProjectTime(Date.parse("2026-06-16T19:28:45.678Z"), "minute"),
    );
  });

  it("keeps an explicitly selected color across unrelated draft edits", async () => {
    const element = await mount(null);
    const color = PROJECT_COLORS[0];
    element.querySelector<HTMLButtonElement>(`button[aria-label="Choose project color ${color}"]`)!.click();
    await element.updateComplete;
    const name = element.querySelector<HTMLInputElement>('input[placeholder="Trip 2026"]')!;
    name.value = "Selected color";
    name.dispatchEvent(new Event("input", { bubbles: true }));
    await element.updateComplete;
    expect(
      element
        .querySelector(`button[aria-label="Choose project color ${color}"]`)
        ?.getAttribute("aria-pressed"),
    ).toBe("true");
  });

  it("does not replace an existing project's range with prefill", async () => {
    const snapshot = demoSnapshot();
    const projectId = snapshot.projects.find((project) => !project.archived)!.id;
    const element = await mount(projectId);
    const project = store.snapshot!.projects.find((p) => p.id === projectId)!;
    expect(element.querySelector('button[aria-pressed="true"]')?.getAttribute("aria-label")).toBe(
      `Choose project color ${project.color}`,
    );
    expect(
      element.querySelector<HTMLInputElement>('[aria-label="Inclusive capture range start"]')!.value,
    ).toBe(formatProjectTime(project.start_time, project.granularity ?? "minute"));
  });
});

describe("project dialog capture-time inputs", () => {
  it("formats and parses minute and day bounds using the machine timezone", () => {
    const timestamp = new Date(2026, 5, 15, 18, 27).getTime();
    expect(formatProjectTime(timestamp, "minute")).toBe("2026-06-15T18:27");
    expect(parseProjectTime("2026-06-15T18:27", "minute")).toBe(timestamp);
    expect(formatProjectTime(timestamp, "day")).toBe("2026-06-15");
    expect(parseProjectTime("2026-06-15", "day")).toBe(new Date(2026, 5, 15).getTime());
    expect(parseProjectTime("2026-06-15", "day", true)).toBe(new Date(2026, 5, 16).getTime() - 1);
  });

  it("uses the start of the selected local year and supports empty bounds", () => {
    const yearStart = new Date(2026, 0, 1).getTime();
    expect(formatProjectTime(yearStart, "year")).toBe("2026");
    expect(parseProjectTime("2026", "year")).toBe(yearStart);
    expect(parseProjectTime("", "year")).toBeNull();
    expect(() => parseProjectTime("26", "year")).toThrow("Invalid");
  });
});
