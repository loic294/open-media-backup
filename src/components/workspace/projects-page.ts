import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { Project, ProjectGranularity } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { newProject } from "../../state/factories";
import { nextProjectColor, PROJECT_COLORS } from "../../state/projects";
import { spaceProjects } from "../../state/selectors";
import { DialogBase } from "../dialogs/dialog-base";
import { formatProjectTime, parseProjectTime, projectVariablesSection } from "../dialogs/project-dialog";

function dateLabel(value: number | null | undefined): string {
  return value == null ? "Any time" : new Date(value).toISOString().slice(0, 10);
}

function projectColor(project: Project): string {
  const color = project.color;
  return color && /^#[0-9a-f]{6}$/i.test(color) ? color : "var(--color-primary)";
}

@customElement("omb-projects-page")
export class OmbProjectsPage extends DialogBase<Extract<DialogRequest, { type: "projects" }>> {
  @state() private draft: Project | null = null;
  @state() private saving = false;

  override connectedCallback(): void {
    super.connectedCallback();
    const { snapshot, space } = this.store;
    const projects = snapshot && space ? spaceProjects(snapshot, space.id) : [];
    const initial = projects[0];
    if (initial) this.draft = structuredClone(initial);
  }

  #create() {
    const { snapshot, space } = this.store;
    if (!snapshot || !space) return;
    this.draft = {
      ...newProject(space, ""),
      color: nextProjectColor(snapshot.projects.filter((project) => project.space_id === space.id)),
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

  override render() {
    const { snapshot, space } = this.store;
    if (!snapshot || !space) return nothing;
    const projects = spaceProjects(snapshot, space.id);
    const draft = this.draft;
    const granularity = draft?.granularity ?? "minute";
    const isNew = !!draft && !snapshot.projects.some((item) => item.id === draft.id);
    const missing = this.#missingRequiredValues();
    const change = (patch: Partial<Project>) => {
      if (draft) this.draft = { ...draft, ...patch };
    };
    const body = html`
      <div class="flex flex-col gap-5">
        <div class="flex items-center justify-end gap-4">
          <button class="btn btn-primary" @click=${() => this.#create()}>
            <omb-icon name="plus"></omb-icon>Create project
          </button>
        </div>
        <div class="grid grid-cols-1 xl:grid-cols-2 gap-5 items-stretch">
          <section class="card bg-base-200 min-h-96" aria-label=${`Projects in ${space.name}`}>
            <div class="card-body p-4">
              <div class="flex items-center gap-3">
                <h2 class="card-title text-base">Projects in ${space.name}</h2>
                <span class="badge badge-ghost"
                  >${projects.length} ${projects.length === 1 ? "project" : "projects"}</span
                >
              </div>
              ${
                projects.length
                  ? projects.map((item) => {
                      const selected = item.id === draft?.id;
                      return html`<div
                        class="rounded-field border p-3 flex items-center gap-3 min-h-20 ${selected ? "border-primary bg-primary/10" : "border-base-300"}"
                      >
                        <span
                          class="w-1 self-stretch rounded-full shrink-0"
                          style=${`background-color:${projectColor(item)}`}
                        ></span>
                        <button
                          class="text-left flex-1 min-w-0 hover:opacity-80"
                          aria-pressed=${selected}
                          @click=${() => (this.draft = structuredClone(item))}
                        >
                          <span class="font-semibold block truncate">${item.name || "Untitled project"}</span>
                          <span class="text-xs text-base-content/70 block mt-1">
                            ${dateLabel(item.start_time)} – ${dateLabel(item.end_time)} ·
                            ${item.granularity ?? "minute"} grouping
                          </span>
                          ${item.archived ? html`<span class="badge badge-ghost badge-xs mt-1">Archived</span>` : nothing}
                        </button>
                      </div>`;
                    })
                  : html`<p class="text-sm text-base-content/60 py-8 text-center">
                      No projects in this space yet.
                    </p>`
              }
              <p class="text-xs text-base-content/60 rounded-field bg-base-100 p-3">
                Files match projects by embedded capture time and source scope. Non-media files with the same
                filename stem in the same folder inherit a dated media file's capture time. Files without a
                usable capture time remain unassigned.
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
                        ${(["start_time", "end_time"] as const).map(
                          (key) =>
                            html`<label class="fieldset">
                              <span class="fieldset-legend"
                                >${key === "start_time" ? "Start" : "End · inclusive"}</span
                              >
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
                            </label>`,
                        )}
                      </div>
                    </fieldset>
                    <fieldset class="fieldset">
                      <legend class="fieldset-legend">Grouping granularity</legend>
                      <div class="join w-fit">
                        ${(["minute", "day", "year"] as const).map(
                          (value) =>
                            html`<button
                              class="btn btn-sm join-item ${granularity === value ? "btn-active btn-primary" : ""}"
                              aria-pressed=${granularity === value}
                              @click=${() => change({ granularity: value })}
                            >
                              ${value[0].toUpperCase()}${value.slice(1)}
                            </button>`,
                        )}
                      </div>
                    </fieldset>
                    <fieldset class="fieldset">
                      <legend class="fieldset-legend">Project color</legend>
                      <div
                        class="grid grid-cols-8 gap-2 w-fit"
                        role="group"
                        aria-label="Predefined project colors"
                      >
                        ${PROJECT_COLORS.map(
                          (color) =>
                            html`<button
                              class="size-6 rounded-full border-2 ${draft.color?.toLowerCase() === color.toLowerCase() ? "border-base-content ring-2 ring-primary ring-offset-2 ring-offset-base-200" : "border-transparent"}"
                              style=${`background-color:${color}`}
                              aria-label=${`Choose project color ${color}`}
                              aria-pressed=${draft.color?.toLowerCase() === color.toLowerCase()}
                              @click=${() => change({ color })}
                            ></button>`,
                        )}
                      </div>
                      <p class="label">Random default, avoiding the last three projects in this space.</p>
                    </fieldset>
                    ${projectVariablesSection(space, draft, (name, value) =>
                      change({ values: { ...draft.values, [name]: value } }),
                    )}
                    <div class="flex items-center justify-between gap-3 pt-2">
                      <label class="flex items-center gap-2 text-sm">
                        <input
                          type="checkbox"
                          class="toggle toggle-sm"
                          .checked=${draft.archived}
                          @change=${(event: Event) => change({ archived: (event.target as HTMLInputElement).checked })}
                        />
                        Archived
                      </label>
                    </div>
                  </div>`
                : html`<div class="card-body place-items-center text-center">
                    <p class="text-sm text-base-content/60">
                      Select a project to edit its range and settings.
                    </p>
                  </div>`
            }
          </section>
        </div>
      </div>
    `;
    const actions = html`
      ${missing.length ? html`<span class="text-xs text-error mr-2">Missing ${missing.join(", ")}</span>` : nothing}
      <button class="btn btn-ghost" @click=${() => this.dismiss()}>Cancel</button>
      <button
        class="btn btn-primary"
        ?disabled=${this.saving || !draft?.name.trim() || missing.length > 0}
        @click=${() => this.#save()}
      >
        ${this.saving ? html`<span class="loading loading-spinner loading-sm"></span>` : nothing}
        ${isNew ? "Create project" : "Save project"}
      </button>
    `;
    return html`<omb-modal
      size="screen"
      heading="Projects"
      subheading=${`${space.name} · manage project ranges and variables`}
      icon="folder"
      @close=${this.onClosed}
      .body=${body}
      .actions=${actions}
    ></omb-modal>`;
  }
}
