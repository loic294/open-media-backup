import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { Project, ProjectGranularity } from "../../api/types";
import { newProject } from "../../state/factories";
import { nextProjectColor, PROJECT_COLORS } from "../../state/projects";
import { spaceProjects } from "../../state/selectors";
import { formatProjectTime, parseProjectTime } from "../dialogs/project-dialog";
import { OmbElement } from "../ui/omb-element";

function dateLabel(value: number | null | undefined): string {
  return value == null ? "Any time" : new Date(value).toISOString().slice(0, 10);
}

function projectColor(project: Project): string {
  const color = project.color;
  return color && /^#[0-9a-f]{6}$/i.test(color) ? color : "var(--color-primary)";
}

@customElement("omb-projects-page")
export class OmbProjectsPage extends OmbElement {
  @state() private draft: Project | null = null;
  @state() private saving = false;

  override connectedCallback(): void {
    super.connectedCallback();
    const { project, snapshot, space } = this.store;
    const projects = snapshot && space ? spaceProjects(snapshot, space.id) : [];
    const initial = projects.find((item) => item.id === project?.id) ?? projects[0];
    if (initial) this.draft = structuredClone(initial);
  }

  #create() {
    const { snapshot, space } = this.store;
    if (!snapshot || !space) return;
    this.draft = {
      ...newProject(space, ""),
      color: nextProjectColor(spaceProjects(snapshot, space.id)),
    };
  }

  async #save() {
    if (!this.draft || this.saving) return;
    this.saving = true;
    try {
      const saved = await this.store.save("project", {
        ...this.draft,
        name: this.draft.name.trim(),
      });
      if (!saved) return;
      this.draft = structuredClone(this.store.snapshot!.projects.find((item) => item.id === this.draft!.id)!);
      if (!this.store.project || this.draft.id !== this.store.project.id) {
        await this.store.selectProject(this.draft.id);
      }
      this.store.toast("success", "Project saved");
    } finally {
      this.saving = false;
    }
  }

  #delete() {
    if (!this.draft) return;
    const id = this.draft.id;
    this.store.open({
      type: "confirm",
      title: `Delete “${this.draft.name}”?`,
      message: "The project and its variable values are removed. Files and backup history are kept.",
      confirmLabel: "Delete project",
      danger: true,
      onConfirm: async () => {
        await this.store.remove("project", id);
        const { snapshot, space } = this.store;
        const next = snapshot && space ? spaceProjects(snapshot, space.id)[0] : undefined;
        this.draft = next ? structuredClone(next) : null;
      },
    });
  }

  #updateTime(key: "start_time" | "end_time", value: string, granularity: ProjectGranularity) {
    if (this.draft) this.draft = { ...this.draft, [key]: parseProjectTime(value, granularity) };
  }

  #missingRequiredValues(): string[] {
    if (!this.draft || !this.store.space) return [];
    return this.store.space.variables
      .filter(
        (variable) =>
          variable.required &&
          !(this.draft!.values[variable.name] ?? "").trim() &&
          !(variable.name === "project_name" && this.draft!.name.trim()),
      )
      .map((variable) => variable.name);
  }

  #timeline(projects: Project[]) {
    const ranged = projects.filter((project) => project.start_time != null && project.end_time != null);
    if (!ranged.length) {
      return html`<section class="card bg-base-200" aria-label="Project timeline">
        <div class="card-body p-4">
          <h2 class="card-title text-base">Project timeline</h2>
          <p class="rounded-box bg-base-100 p-6 text-sm text-base-content/60">
            Set inclusive capture-time ranges on projects to place them on the timeline.
          </p>
        </div>
      </section>`;
    }
    const min = Math.min(...ranged.map((project) => project.start_time!));
    const max = Math.max(...ranged.map((project) => project.end_time!));
    const span = Math.max(max - min, 60_000);
    const ticks = Array.from({ length: 6 }, (_, index) => min + (span * index) / 5);
    return html`<section class="card bg-base-200" aria-label="Project timeline">
      <div class="card-body p-4">
        <div class="flex items-center gap-3 flex-wrap">
          <h2 class="card-title text-base">Project timeline</h2>
          <p class="text-xs text-base-content/60">
            Each colored span is an inclusive capture-time range; overlaps can match multiple projects.
          </p>
        </div>
        <div class="flex justify-between text-xs text-base-content/50 mt-2" aria-hidden="true">
          ${ticks.map((time) => html`<span>${new Date(time).toISOString().slice(0, 10)}</span>`)}
        </div>
        <div class="flex flex-col gap-2 p-3 rounded-field bg-base-100">
          ${ranged.map((project) => {
            const left = ((project.start_time! - min) / span) * 100;
            const width = Math.max(((project.end_time! - project.start_time!) / span) * 100, 1.5);
            const color = projectColor(project);
            const colorBackground = color.startsWith("#")
              ? `${color}35`
              : "color-mix(in srgb, var(--color-primary) 20%, transparent)";
            return html`<button
              class="relative h-9 w-full rounded-field text-left border border-base-300 hover:border-base-content/50"
              aria-label=${`${project.name}: ${dateLabel(project.start_time)} to ${dateLabel(project.end_time)}`}
              @click=${() => (this.draft = structuredClone(project))}
            >
              <span
                class="absolute inset-y-0 rounded-field px-2 flex items-center truncate text-xs font-medium"
                style=${`left:${left}%;width:${Math.min(width, 100 - left)}%;background-color:${colorBackground};border:1px solid ${color};color:${color}`}
              >${project.name}</span>
            </button>`;
          })}
        </div>
      </div>
    </section>`;
  }

  override render() {
    const { snapshot, space, project: activeProject } = this.store;
    if (!snapshot || !space) return nothing;
    const projects = spaceProjects(snapshot, space.id);
    const draft = this.draft;
    const granularity = draft?.granularity ?? "minute";
    const isNew = !!draft && !snapshot.projects.some((item) => item.id === draft.id);
    const missing = this.#missingRequiredValues();
    const change = (patch: Partial<Project>) => {
      if (draft) this.draft = { ...draft, ...patch };
    };
    return html`<main class="fixed inset-0 z-40 overflow-y-auto bg-base-100 text-base-content">
      <header class="navbar min-h-16 px-6 border-b border-base-300 bg-base-200">
        <div class="flex-1 items-center gap-3">
          <span class="font-semibold">Open Media Backup</span>
          <span class="text-base-content/40">/</span>
          <span class="text-sm">${space.name}</span>
        </div>
        <button class="btn btn-ghost btn-sm" @click=${() => this.store.closeProjectsPage()}>
          <omb-icon name="arrow-left"></omb-icon>Back to workspace
        </button>
      </header>
      <div class="max-w-screen-2xl mx-auto p-6 lg:p-8 flex flex-col gap-5">
        <div class="flex items-center justify-between gap-4">
          <div>
            <h1 class="text-2xl font-bold">Projects</h1>
            <p class="text-sm text-base-content/60 mt-1">
              Group media by inclusive capture-time ranges. Project colors identify matching files across sources.
            </p>
          </div>
          <button class="btn btn-primary" @click=${() => this.#create()}>
            <omb-icon name="plus"></omb-icon>Create project
          </button>
        </div>
        ${this.#timeline(projects)}
        <div class="grid grid-cols-1 xl:grid-cols-2 gap-5 items-stretch">
          <section class="card bg-base-200 min-h-96" aria-label=${`Projects in ${space.name}`}>
            <div class="card-body p-4">
              <div class="flex items-center gap-3">
                <h2 class="card-title text-base">Projects in ${space.name}</h2>
                <span class="badge badge-ghost">${projects.length} ${projects.length === 1 ? "project" : "projects"}</span>
              </div>
              ${
                projects.length
                  ? projects.map((item) => {
                      const selected = item.id === draft?.id;
                    return html`<div
                      class="rounded-field border p-3 flex items-center gap-3 min-h-20 ${selected ? "border-primary bg-primary/10" : "border-base-300"}"
                    >
                      <span class="w-1 self-stretch rounded-full shrink-0" style=${`background-color:${projectColor(item)}`}></span>
                      <button
                        class="text-left flex-1 min-w-0 hover:opacity-80"
                        aria-pressed=${selected}
                        @click=${() => (this.draft = structuredClone(item))}
                      >
                        <span class="font-semibold block truncate">${item.name || "Untitled project"}</span>
                        <span class="text-xs text-base-content/70 block mt-1">
                          ${dateLabel(item.start_time)} – ${dateLabel(item.end_time)} · ${item.granularity ?? "minute"} grouping
                        </span>
                        ${item.archived ? html`<span class="badge badge-ghost badge-xs mt-1">Archived</span>` : nothing}
                      </button>
                      ${item.id === activeProject?.id ? html`<span class="badge badge-primary badge-soft badge-sm">Active</span>` : nothing}
                      <button
                        class="btn btn-xs"
                        ?disabled=${item.id === activeProject?.id}
                        aria-label=${`Use ${item.name} project`}
                        @click=${() => void this.store.selectProject(item.id)}
                      >${item.id === activeProject?.id ? "Active" : "Use"}</button>
                    </div>`;
                  })
                  : html`<p class="text-sm text-base-content/60 py-8 text-center">No projects in this space yet.</p>`
              }
              <p class="text-xs text-base-content/60 rounded-field bg-base-100 p-3">
                Files match projects by embedded capture time and source scope. Files without a usable capture time remain unassigned.
              </p>
            </div>
          </section>
          <section class="card bg-base-200 min-h-96" aria-label="Edit project">
            ${
              draft
                ? html`<div class="card-body p-4 gap-4">
                    <div class="flex items-center justify-between">
                      <h2 class="card-title text-base">${isNew ? "New project" : "Edit project"}</h2>
                      ${!isNew ? html`<button class="btn btn-ghost btn-xs text-error" @click=${() => this.#delete()}>Delete</button>` : nothing}
                    </div>
                    <fieldset class="fieldset">
                      <legend class="fieldset-legend">Project name</legend>
                      <input
                        class="input input-sm w-full"
                        aria-label="Project name"
                        .value=${draft.name}
                        @input=${(event: Event) => change({ name: (event.target as HTMLInputElement).value })}
                      />
                    </fieldset>
                    <fieldset class="fieldset">
                      <legend class="fieldset-legend">Inclusive capture range (UTC)</legend>
                      <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                        ${(["start_time", "end_time"] as const).map((key) => html`<label class="fieldset">
                          <span class="fieldset-legend">${key === "start_time" ? "Start" : "End · inclusive"}</span>
                          <input
                            class="input input-sm w-full"
                            type=${granularity === "minute" ? "datetime-local" : granularity === "day" ? "date" : "number"}
                            min=${granularity === "year" ? "1" : nothing}
                            max=${granularity === "year" ? "9999" : nothing}
                            step=${granularity === "year" ? "1" : nothing}
                            .value=${formatProjectTime(draft[key], granularity)}
                            aria-label=${key === "start_time" ? "Inclusive capture range start" : "Inclusive capture range end"}
                            @input=${(event: Event) => this.#updateTime(key, (event.target as HTMLInputElement).value, granularity)}
                          />
                        </label>`)}
                      </div>
                    </fieldset>
                    <fieldset class="fieldset">
                      <legend class="fieldset-legend">Grouping granularity</legend>
                      <div class="join w-fit">
                        ${(["minute", "day", "year"] as const).map((value) => html`<button
                          class="btn btn-sm join-item ${granularity === value ? "btn-active btn-primary" : ""}"
                          aria-pressed=${granularity === value}
                          @click=${() => change({ granularity: value })}
                        >${value[0].toUpperCase()}${value.slice(1)}</button>`)}
                      </div>
                    </fieldset>
                    <fieldset class="fieldset">
                      <legend class="fieldset-legend">Project color</legend>
                      <div class="grid grid-cols-8 gap-2 w-fit" role="group" aria-label="Predefined project colors">
                        ${PROJECT_COLORS.map((color) => html`<button
                          class="size-6 rounded-full border-2 ${draft.color?.toLowerCase() === color.toLowerCase() ? "border-base-content ring-2 ring-primary ring-offset-2 ring-offset-base-200" : "border-transparent"}"
                          style=${`background-color:${color}`}
                          aria-label=${`Choose project color ${color}`}
                          aria-pressed=${draft.color?.toLowerCase() === color.toLowerCase()}
                          @click=${() => change({ color })}
                        ></button>`)}
                      </div>
                      <p class="label">New projects choose the next unused palette color.</p>
                    </fieldset>
                    <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                      ${space.variables.map((variable) => html`<fieldset class="fieldset">
                        <legend class="fieldset-legend font-mono">{${variable.name}}${variable.required ? " *" : ""}</legend>
                        <input
                          class="input input-sm w-full"
                          .value=${draft.values[variable.name] ?? ""}
                          placeholder=${variable.default_value || ""}
                          aria-label=${`Project variable ${variable.name}`}
                          @input=${(event: Event) => change({ values: { ...draft.values, [variable.name]: (event.target as HTMLInputElement).value } })}
                        />
                      </fieldset>`)}
                    </div>
                    <div class="flex items-center justify-between gap-3 pt-2">
                      <label class="flex items-center gap-2 text-sm">
                        <input type="checkbox" class="toggle toggle-sm" .checked=${draft.archived} @change=${(event: Event) => change({ archived: (event.target as HTMLInputElement).checked })} />
                        Archived
                      </label>
                      <button class="btn btn-primary" ?disabled=${this.saving || !draft.name.trim() || missing.length > 0} @click=${() => this.#save()}>
                        ${this.saving ? html`<span class="loading loading-spinner loading-sm"></span>` : nothing}
                        ${isNew ? "Create project" : "Save project"}
                      </button>
                    </div>
                  </div>`
                : html`<div class="card-body place-items-center text-center">
                    <p class="text-sm text-base-content/60">Select a project to edit its range and settings.</p>
                  </div>`
            }
          </section>
        </div>
      </div>
    </main>`;
  }
}
