import { afterEach, describe, expect, it, vi } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { nextProjectColor } from "../../state/projects";
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
  let page: OmbProjectsPage | undefined;
  let picker: OmbProjectPicker | undefined;

  afterEach(() => {
    page?.remove();
    picker?.remove();
    store.snapshot = previousSnapshot;
    store.projectsPageOpen = false;
    vi.restoreAllMocks();
  });

  it("replaces the project dropdown with a button that opens project management", async () => {
    store.snapshot = demoSnapshot();
    store.projectsPageOpen = false;
    picker = new OmbProjectPicker();
    document.body.append(picker);
    await picker.updateComplete;
    expect(picker.querySelector("details.dropdown")).toBeNull();
    [...picker.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("Projects"))!
      .click();
    expect(store.projectsPageOpen).toBe(true);
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
    await until(() => !!page!.querySelector('button[aria-label="Create project"]') || page!.textContent?.includes("Projects"));
    const existing = store.snapshot!.projects.filter(
      (project) => project.space_id === store.space!.id,
    );
    const expectedColor = nextProjectColor(existing);
    [...page.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.trim() === "Create project")!
      .click();
    await page.updateComplete;
    const name = page.querySelector<HTMLInputElement>('input[aria-label="Project name"]') ??
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
