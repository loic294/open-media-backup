import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { Project, ProjectGranularity, Space } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { newProject } from "../../state/factories";
import { nextProjectColor, PROJECT_COLORS } from "../../state/projects";
import { deviceById, mappingFor, spaceDestinations } from "../../state/selectors";
import { expandTemplate, previewVars } from "../../utils/template";
import { DialogBase } from "./dialog-base";

export function formatProjectTime(value: number | null | undefined, granularity: ProjectGranularity): string {
  if (value == null) return "";
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return "";
  if (granularity === "year") return String(date.getUTCFullYear()).padStart(4, "0");
  if (granularity === "day") return date.toISOString().slice(0, 10);
  return date.toISOString().slice(0, 16);
}

export function parseProjectTime(value: string, granularity: ProjectGranularity): number | null {
  if (!value) return null;
  if (granularity === "year") {
    if (!/^\d{4}$/.test(value)) return null;
    const year = Number(value);
    if (year < 1 || year > 9999) return null;
    const date = new Date(0);
    date.setUTCFullYear(year, 0, 1);
    date.setUTCHours(0, 0, 0, 0);
    return date.getTime();
  }
  const timestamp = Date.parse(granularity === "day" ? `${value}T00:00:00.000Z` : `${value}Z`);
  return Number.isFinite(timestamp) ? timestamp : null;
}

export function projectVariablesSection(
  space: Space,
  draft: Project,
  setValue: (name: string, value: string) => void,
) {
  return html`<section class="card card-border bg-base-100">
    <div class="card-body p-4 gap-4">
      <div>
        <h4 class="card-title text-base">Project variables</h4>
        <p class="text-sm text-base-content/60">
          Values are used anywhere this space's source and destination templates reference a variable.
        </p>
      </div>
      ${
        space.variables.length === 0
          ? html`<p class="text-sm text-base-content/60">
              This space has no variables. Add some in the space settings.
            </p>`
          : html`<div class="grid grid-cols-1 gap-3">
              ${space.variables.map((variable) => {
                const fallback =
                  variable.default_value ||
                  (variable.name === "project_name" ? draft.name || "defaults to the project name" : "");
                const hint = variable.required
                  ? fallback
                    ? `Required · Default: ${fallback}`
                    : "Required"
                  : fallback
                    ? `Default: ${fallback}`
                    : "Optional";
                return html`<fieldset class="fieldset">
                  <legend class="fieldset-legend font-mono">
                    ${variable.name}${variable.required ? html`<span class="text-error">*</span>` : nothing}
                  </legend>
                  <input
                    class="input w-full"
                    .value=${draft.values[variable.name] ?? ""}
                    placeholder=${fallback}
                    aria-label=${`Project variable ${variable.name}`}
                    @input=${(e: Event) => setValue(variable.name, (e.target as HTMLInputElement).value)}
                  />
                  <p class="label">${hint}</p>
                </fieldset>`;
              })}
            </div>`
      }
    </div>
  </section>`;
}

/** Create or edit a project: name, a value for each space variable, and wipe safety. */
@customElement("omb-project-dialog")
export class OmbProjectDialog extends DialogBase<Extract<DialogRequest, { type: "project" }>> {
  @state() private draft!: Project;

  override connectedCallback(): void {
    super.connectedCallback();
    const existing = this.store.snapshot!.projects.find((p) => p.id === this.request.projectId);
    this.draft = existing ? structuredClone(existing) : newProject(this.store.space!, "");
    if (!existing) {
      this.draft.color = nextProjectColor(
        this.store.snapshot!.projects.filter((project) => project.space_id === this.store.space!.id),
      );
    }
    if (
      !this.request.projectId &&
      "start_time" in this.request &&
      "end_time" in this.request &&
      typeof this.request.start_time === "number" &&
      typeof this.request.end_time === "number"
    ) {
      this.draft = {
        ...this.draft,
        start_time: this.request.start_time,
        end_time: this.request.end_time,
        granularity: "minute",
      };
    }
  }

  get #isNew() {
    return !this.request.projectId;
  }

  get #missing() {
    const space = this.store.space!;
    return space.variables.filter(
      (v) =>
        v.required &&
        !(this.draft.values[v.name] ?? "").trim() &&
        !(v.name === "project_name" && this.draft.name.trim()),
    );
  }

  async #save() {
    const previous = this.store.snapshot;
    await this.store.save("project", { ...this.draft, name: this.draft.name.trim() });
    if (this.store.snapshot === previous) return;
    if (this.#isNew) await this.store.selectProject(this.draft.id);
    this.dismiss();
  }

  #delete() {
    this.store.open({
      type: "confirm",
      title: `Delete “${this.draft.name}”?`,
      message: "The project and its variable values are removed. Files and backup history are kept.",
      confirmLabel: "Delete project",
      danger: true,
      onConfirm: async () => {
        await this.store.remove("project", this.draft.id);
        this.dismiss();
      },
    });
  }

  override render() {
    const { snapshot, space } = this.store;
    if (!snapshot || !space || !this.draft) return nothing;
    const d = this.draft;
    const setValue = (name: string, value: string) =>
      (this.draft = { ...d, values: { ...d.values, [name]: value } });
    const granularity = d.granularity ?? "minute";
    const updateTime = (key: "start_time" | "end_time", value: string) =>
      (this.draft = { ...d, [key]: parseProjectTime(value, granularity) });
    const vars = previewVars(space, d);
    const body = html`
      <div class="flex flex-col gap-5">
        <fieldset class="fieldset">
          <legend class="fieldset-legend">Project name</legend>
          <input
            class="input w-full"
            autofocus
            .value=${d.name}
            placeholder="Trip 2026"
            @input=${(e: Event) => (this.draft = { ...d, name: (e.target as HTMLInputElement).value })}
          />
        </fieldset>
        <section class="flex flex-col gap-3">
          <div>
            <h4 class="font-medium">Capture-time range</h4>
            <p class="text-sm text-base-content/60">
              Both bounds are inclusive and interpreted in UTC. Leave both empty to match any capture time.
            </p>
          </div>
          <fieldset class="fieldset">
            <legend class="fieldset-legend">UTC bucket granularity</legend>
            <select
              class="select w-full"
              .value=${granularity}
              @change=${(e: Event) => (this.draft = { ...d, granularity: (e.target as HTMLSelectElement).value as ProjectGranularity })}
            >
              <option value="minute">Minute</option>
              <option value="day">Day</option>
              <option value="year">Year</option>
            </select>
          </fieldset>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
            ${(["start_time", "end_time"] as const).map(
              (key) =>
                html`<fieldset class="fieldset">
                  <legend class="fieldset-legend">
                    ${key === "start_time" ? "Inclusive start" : "Inclusive end"}
                  </legend>
                  <input
                    class="input w-full"
                    type=${granularity === "minute" ? "datetime-local" : granularity === "day" ? "date" : "number"}
                    min=${granularity === "year" ? "1" : nothing}
                    max=${granularity === "year" ? "9999" : nothing}
                    step=${granularity === "year" ? "1" : nothing}
                    .value=${formatProjectTime(d[key], granularity)}
                    aria-label=${key === "start_time" ? "Inclusive capture range start" : "Inclusive capture range end"}
                    @input=${(e: Event) => updateTime(key, (e.target as HTMLInputElement).value)}
                  />
                </fieldset>`,
            )}
          </div>
          <p class="text-xs text-base-content/60">
            Day and year buckets include the entire selected UTC day or year.
          </p>
          <fieldset class="fieldset">
            <legend class="fieldset-legend">Project color</legend>
            <div class="grid grid-cols-8 gap-2 w-fit" role="group" aria-label="Predefined project colors">
              ${PROJECT_COLORS.map(
                (color) =>
                  html`<button
                    type="button"
                    class="size-6 rounded-full border-2 transition-transform hover:scale-110 ${d.color?.toLowerCase() === color.toLowerCase() ? "border-base-content ring-2 ring-primary ring-offset-2 ring-offset-base-100" : "border-transparent"}"
                    style=${`background-color:${color}`}
                    aria-label=${`Choose project color ${color}`}
                    aria-pressed=${d.color?.toLowerCase() === color.toLowerCase()}
                    @click=${() => (this.draft = { ...d, color })}
                  ></button>`,
              )}
            </div>
            <p class="label">New projects use the next unused palette color.</p>
            ${
              d.color && !PROJECT_COLORS.some((color) => color.toLowerCase() === d.color?.toLowerCase())
                ? html`<p class="text-xs text-base-content/60">Current color: ${d.color}</p>`
                : nothing
            }
          </fieldset>
        </section>
        ${projectVariablesSection(space, d, setValue)}
        <section>
          <h4 class="font-medium mb-1">Resulting destination folders</h4>
          <ul class="text-xs font-mono flex flex-col gap-1">
            ${spaceDestinations(snapshot, space.id).map((dest) => {
              const device = deviceById(snapshot, dest.device_id);
              const root = mappingFor(snapshot, dest.device_id)?.root_path ?? device?.name ?? "?";
              return html`<li class="truncate">
                <span class="text-base-content/50">${device?.name}:</span>
                ${root}/${expandTemplate(dest.path_template, vars)}
              </li>`;
            })}
          </ul>
        </section>
        <section class="grid grid-cols-2 gap-4">
          <fieldset class="fieldset">
            <legend class="fieldset-legend">Final copies before wiping</legend>
            <input
              type="number"
              min="1"
              max="5"
              class="input w-24"
              .value=${String(d.final_copies_required)}
              @input=${(e: Event) => (this.draft = { ...d, final_copies_required: Math.max(1, Number((e.target as HTMLInputElement).value) || 1) })}
            />
            <p class="label">
              Cards can be wiped once every file is verified on this many final destinations.
            </p>
          </fieldset>
          ${
            this.#isNew
              ? nothing
              : html`<label class="flex items-center gap-3 cursor-pointer self-center">
                  <input
                    type="checkbox"
                    class="toggle"
                    .checked=${d.archived}
                    @change=${(e: Event) => (this.draft = { ...d, archived: (e.target as HTMLInputElement).checked })}
                  />
                  Archived
                </label>`
          }
        </section>
      </div>
    `;
    const missing = this.#missing;
    const actions = html`
      ${this.#isNew ? nothing : html`<button class="btn btn-ghost text-error mr-auto" @click=${() => this.#delete()}><omb-icon name="trash"></omb-icon>Delete</button>`}
      ${missing.length ? html`<span class="text-xs text-error mr-2">Missing ${missing.map((m) => m.name).join(", ")}</span>` : nothing}
      <button class="btn btn-ghost" @click=${() => this.dismiss()}>Cancel</button>
      <button
        class="btn btn-primary"
        ?disabled=${!d.name.trim() || missing.length > 0}
        @click=${() => this.#save()}
      >
        ${this.#isNew ? "Create project" : "Save"}
      </button>
    `;
    return html`<omb-modal
      heading=${this.#isNew ? "New project" : "Edit project"}
      subheading=${`In ${space.name}`}
      icon="folder-plus"
      @close=${this.onClosed}
      .body=${body}
      .actions=${actions}
    ></omb-modal>`;
  }
}
