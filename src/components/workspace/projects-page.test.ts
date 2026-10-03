import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { nextProjectColor } from "../../state/projects";
import { OmbFlowBoard } from "./flow-board";
import { OmbProjectsPage } from "./projects-page";
import { OmbProjectPicker } from "./project-picker";

async function until(condition: () => boolean) {
  for (let count = 0; count < 100; count++) {
    if (condition()) return;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  throw new Error("Project page did not settle");
}

describe("project management page", () => {
  const previousSnapshot = store.snapshot;
  const previousDialogs = store.dialogs;
  let page: OmbProjectsPage | undefined;
  let picker: OmbProjectPicker | undefined;
  let board: OmbFlowBoard | undefined;

  afterEach(() => {
    page?.remove();
    picker?.remove();
    board?.remove();
    store.snapshot = previousSnapshot;
    store.dialogs = previousDialogs;
    store.projectsPageOpen = false;
    vi.restoreAllMocks();
  });

  it("shows project action buttons without the active project display", async () => {
    store.snapshot = demoSnapshot();
    store.dialogs = [];
    store.projectsPageOpen = false;
    picker = new OmbProjectPicker();
    document.body.append(picker);
    await picker.updateComplete;
    expect(picker.querySelector("details.dropdown")).toBeNull();
    expect(picker.textContent).not.toContain(store.project!.name);
    [...picker.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("Projects"))!
      .click();
    expect(store.projectsPageOpen).toBe(true);
    [...picker.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("New project"))!
      .click();
    expect(store.dialogs.at(-1)).toMatchObject({ type: "project", projectId: null });
  });

  it("omits the canvas connection hint", async () => {
    store.snapshot = demoSnapshot();
    board = new OmbFlowBoard();
    document.body.append(board);
    await board.updateComplete;
    expect(board.textContent).not.toContain("Drag from a source port onto a destination to connect");
  });

  it("creates projects from the management page with the next unused palette color", async () => {
    store.snapshot = demoSnapshot();
    vi.spyOn(store.backend, "saveEntity").mockResolvedValue();
    vi.spyOn(store.backend, "saveSettings").mockResolvedValue();
    vi.spyOn(store.backend, "getProjectStatus").mockResolvedValue({
      project_id: "",
      flows: [],
      sources: [],
      destinations: [],
    });
    page = new OmbProjectsPage();
    document.body.append(page);
    await page.updateComplete;
    await until(
      () =>
        !!page!.querySelector('button[aria-label="Create project"]') ||
        page!.textContent?.includes("Projects"),
    );
    const existing = store.snapshot!.projects.filter((project) => project.space_id === store.space!.id);
    const expectedColor = nextProjectColor(existing);
    [...page.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.trim() === "Create project")!
      .click();
    await page.updateComplete;
    const name =
      page.querySelector<HTMLInputElement>('input[aria-label="Project name"]') ??
      page.querySelector<HTMLInputElement>("input");
    expect(name).not.toBeNull();
    name!.value = "New workspace project";
    name!.dispatchEvent(new Event("input", { bubbles: true }));
    await page.updateComplete;
    [...page.querySelectorAll<HTMLButtonElement>("button")]
      .filter((button) => button.textContent?.includes("Create project"))
      .at(-1)!
      .click();
    await until(() => store.project?.name === "New workspace project");
    const created = store.snapshot!.projects.find((project) => project.name === "New workspace project")!;
    expect(created.color).toBe(expectedColor);
    expect(store.project?.id).toBe(created.id);
    expect(page.textContent).toContain("Project timeline");
  });
});
