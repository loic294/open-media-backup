import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { Project } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { newProject } from "../../state/factories";
import { deviceById, mappingFor, spaceDestinations } from "../../state/selectors";
import { expandTemplate, previewVars } from "../../utils/template";
import { DialogBase } from "./dialog-base";

/** Create or edit a project: name, a value for each space variable, and wipe safety. */
@customElement("omb-project-dialog")
export class OmbProjectDialog extends DialogBase<Extract<DialogRequest, { type: "project" }>> {
  @state() private draft!: Project;

  override connectedCallback(): void {
    super.connectedCallback();
    const existing = this.store.snapshot!.projects.find((p) => p.id === this.request.projectId);
    this.draft = existing ? structuredClone(existing) : newProject(this.store.space!, "");
  }

  get #isNew() {
    return !this.request.projectId;
  }

  get #missing() {
    const space = this.store.space!;
    return space.variables.filter((v) => v.required && !(this.draft.values[v.name] ?? "").trim() && !(v.name === "project_name" && this.draft.name.trim()));
  }

  async #save() {
    await this.store.save("project", { ...this.draft, name: this.draft.name.trim() });
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
    const setValue = (name: string, value: string) => (this.draft = { ...d, values: { ...d.values, [name]: value } });
    const vars = previewVars(space, d);
    const body = html`
      <div class="flex flex-col gap-5">
        <fieldset class="fieldset">
          <legend class="fieldset-legend">Project name</legend>
          <input class="input w-full" autofocus .value=${d.name} placeholder="Trip 2026" @input=${(e: Event) => (this.draft = { ...d, name: (e.target as HTMLInputElement).value })} />
        </fieldset>
        <section>
          <h4 class="font-medium mb-1">Variables</h4>
          ${space.variables.length === 0 ? html`<p class="text-sm text-base-content/60">This space has no variables. Add some in the space settings.</p>` : nothing}
          <div class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 items-center">
            ${space.variables.map(
              (v) => html`
                <label class="font-mono text-sm">{${v.name}}${v.required ? html`<span class="text-error">*</span>` : nothing}</label>
                <input
                  class="input input-sm w-full"
                  .value=${d.values[v.name] ?? ""}
                  placeholder=${v.default_value || (v.name === "project_name" ? d.name || "defaults to the project name" : "")}
                  @input=${(e: Event) => setValue(v.name, (e.target as HTMLInputElement).value)}
                />
              `,
            )}
          </div>
        </section>
        <section>
          <h4 class="font-medium mb-1">Resulting destination folders</h4>
          <ul class="text-xs font-mono flex flex-col gap-1">
            ${spaceDestinations(snapshot, space.id).map((dest) => {
              const device = deviceById(snapshot, dest.device_id);
              const root = mappingFor(snapshot, dest.device_id)?.root_path ?? device?.name ?? "?";
              return html`<li class="truncate"><span class="text-base-content/50">${device?.name}:</span> ${root}/${expandTemplate(dest.path_template, vars)}</li>`;
            })}
          </ul>
        </section>
        <section class="grid grid-cols-2 gap-4">
          <fieldset class="fieldset">
            <legend class="fieldset-legend">Final copies before wiping</legend>
            <input type="number" min="1" max="5" class="input w-24" .value=${String(d.final_copies_required)} @input=${(e: Event) => (this.draft = { ...d, final_copies_required: Math.max(1, Number((e.target as HTMLInputElement).value) || 1) })} />
            <p class="label">Cards can be wiped once every file is verified on this many final destinations.</p>
          </fieldset>
          ${this.#isNew
            ? nothing
            : html`<label class="flex items-center gap-3 cursor-pointer self-center">
                <input type="checkbox" class="toggle" .checked=${d.archived} @change=${(e: Event) => (this.draft = { ...d, archived: (e.target as HTMLInputElement).checked })} />
                Archived
              </label>`}
        </section>
      </div>
    `;
    const missing = this.#missing;
    const actions = html`
      ${this.#isNew ? nothing : html`<button class="btn btn-ghost text-error mr-auto" @click=${() => this.#delete()}><omb-icon name="trash"></omb-icon>Delete</button>`}
      ${missing.length ? html`<span class="text-xs text-error mr-2">Missing ${missing.map((m) => m.name).join(", ")}</span>` : nothing}
      <button class="btn btn-ghost" @click=${() => this.dismiss()}>Cancel</button>
      <button class="btn btn-primary" ?disabled=${!d.name.trim() || missing.length > 0} @click=${() => this.#save()}>${this.#isNew ? "Create project" : "Save"}</button>
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
