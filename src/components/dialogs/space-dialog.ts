import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { HashAlgo, Space, VariableDef } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { BUILTIN_VARS, previewVars } from "../../utils/template";
import { HASH_INFO, HASH_LABEL } from "../ui/hash-info";
import { SPACE_ICONS } from "../ui/icons";
import { DialogBase } from "./dialog-base";
import "../form/template-input";

const VAR_NAME = /^[a-zA-Z][a-zA-Z0-9_]*$/;

export function variableErrors(vars: VariableDef[]): string[] {
  const errors: string[] = [];
  const seen = new Set<string>();
  for (const v of vars) {
    if (!VAR_NAME.test(v.name))
      errors.push(`“${v.name || "(empty)"}” must start with a letter and use only letters, digits and _`);
    else if (seen.has(v.name)) errors.push(`“${v.name}” is defined twice`);
    else if (["project", "source_name", "date", "year", "month", "day"].includes(v.name))
      errors.push(`“${v.name}” is a built-in variable`);
    seen.add(v.name);
  }
  return errors;
}

@customElement("omb-space-dialog")
export class OmbSpaceDialog extends DialogBase<Extract<DialogRequest, { type: "space-settings" }>> {
  @state() private draft!: Space;

  override connectedCallback(): void {
    super.connectedCallback();
    this.draft = structuredClone(this.store.snapshot!.spaces.find((s) => s.id === this.request.spaceId)!);
  }

  override firstUpdated(): void {
    const { focus } = this.request;
    if (!focus) return;
    requestAnimationFrame(() => {
      const target =
        focus === "variables"
          ? this.querySelector<HTMLElement>("#omb-space-vars")
          : this.querySelector<HTMLInputElement>("#omb-space-name");
      target?.scrollIntoView({ block: "center" });
      if (target instanceof HTMLInputElement) target.select();
    });
  }

  #setVar(i: number, patch: Partial<VariableDef>) {
    this.draft = {
      ...this.draft,
      variables: this.draft.variables.map((v, j) => (j === i ? { ...v, ...patch } : v)),
    };
  }

  async #save() {
    const previous = this.store.snapshot;
    await this.store.save("space", { ...this.draft, name: this.draft.name.trim() });
    if (this.store.snapshot === previous) return;
    this.dismiss();
  }

  #varsTable() {
    const vars = this.draft.variables;
    return html`
      <table class="table table-sm">
        <thead>
          <tr>
            <th>Name</th>
            <th>Default value</th>
            <th class="text-center">Required</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          ${vars.map(
            (v, i) =>
              html`<tr>
                <td>
                  <input
                    class="input input-sm font-mono w-full"
                    .value=${v.name}
                    placeholder="client"
                    @input=${(e: Event) => this.#setVar(i, { name: (e.target as HTMLInputElement).value.trim() })}
                  />
                </td>
                <td>
                  <input
                    class="input input-sm w-full"
                    .value=${v.default_value}
                    placeholder="optional"
                    @input=${(e: Event) => this.#setVar(i, { default_value: (e.target as HTMLInputElement).value })}
                  />
                </td>
                <td class="text-center">
                  <input
                    type="checkbox"
                    class="checkbox checkbox-sm"
                    .checked=${v.required}
                    @change=${(e: Event) => this.#setVar(i, { required: (e.target as HTMLInputElement).checked })}
                  />
                </td>
                <td>
                  <button
                    class="btn btn-ghost btn-xs btn-square"
                    @click=${() => (this.draft = { ...this.draft, variables: vars.filter((_, j) => j !== i) })}
                  >
                    <omb-icon name="x"></omb-icon>
                  </button>
                </td>
              </tr>`,
          )}
        </tbody>
      </table>
      <button
        class="btn btn-sm btn-ghost gap-1 mt-1"
        @click=${() => (this.draft = { ...this.draft, variables: [...vars, { name: "", default_value: "", required: false }] })}
      >
        <omb-icon name="plus"></omb-icon>Add variable
      </button>
      <p class="text-xs text-base-content/60 mt-2">
        Use them as <code>{name}</code> in source and destination folders. Always available:
        ${BUILTIN_VARS.filter((b) => !vars.some((v) => v.name === b)).map((b) => html`<code class="mx-0.5">{${b}}</code>`)}
      </p>
    `;
  }

  #hashChoice(algo: HashAlgo) {
    const info = HASH_INFO[algo];
    const checked = this.draft.hash_algo === algo;
    return html`<label
      class="flex-1 rounded-box border p-3 cursor-pointer ${checked ? "border-primary bg-primary/5" : "border-base-300"}"
    >
      <div class="flex items-center gap-2 font-medium mb-2">
        <input
          type="radio"
          name="omb-hash"
          class="radio radio-sm radio-primary"
          .checked=${checked}
          @change=${() => (this.draft = { ...this.draft, hash_algo: algo })}
        />
        ${HASH_LABEL[algo]}${algo === "xxh64" ? html`<span class="badge badge-sm badge-soft badge-primary">default</span>` : nothing}
      </div>
      <ul class="text-xs space-y-1">
        ${info.pros.map((p) => html`<li class="flex gap-1.5"><omb-icon name="check" class="size-3.5 text-success mt-0.5"></omb-icon>${p}</li>`)}
        ${info.cons.map((c) => html`<li class="flex gap-1.5 text-base-content/60"><omb-icon name="x" class="size-3.5 text-error mt-0.5"></omb-icon>${c}</li>`)}
      </ul>
    </label>`;
  }

  override render() {
    if (!this.draft) return nothing;
    const d = this.draft;
    const errors = variableErrors(d.variables);
    const body = html`
      <div class="flex flex-col gap-6">
        <div class="grid grid-cols-[1fr_auto] gap-4 items-end">
          <fieldset class="fieldset">
            <legend class="fieldset-legend">Name</legend>
            <input
              id="omb-space-name"
              class="input w-full"
              .value=${d.name}
              @input=${(e: Event) => (this.draft = { ...d, name: (e.target as HTMLInputElement).value })}
            />
          </fieldset>
          <div class="join">
            ${SPACE_ICONS.map(
              (icon) =>
                html`<button
                  class="btn btn-sm btn-square join-item ${d.icon === icon ? "btn-primary" : ""}"
                  title=${icon}
                  @click=${() => (this.draft = { ...d, icon })}
                >
                  <omb-icon name=${icon}></omb-icon>
                </button>`,
            )}
          </div>
        </div>
        <section id="omb-space-vars">
          <h4 class="font-medium">Project variables</h4>
          <p class="text-sm text-base-content/60 mb-2">
            Each project of this space gives its own value to these variables.
          </p>
          ${this.#varsTable()} ${errors.map((e) => html`<p class="text-xs text-error">${e}</p>`)}
        </section>
        <section>
          <h4 class="font-medium mb-2">Hash algorithm</h4>
          <div class="flex gap-3">${this.#hashChoice("xxh64")}${this.#hashChoice("blake3")}</div>
        </section>
        <section>
          <h4 class="font-medium mb-2">Verification</h4>
          <div class="flex flex-col gap-2">
            ${(
              [
                [
                  "inline",
                  "Hash while copying",
                  "Fast: source is hashed during the copy and the written file is flushed to disk.",
                ],
                [
                  "reread",
                  "Re-read after copying",
                  "Slower but strongest: every copy is read back from the destination and compared.",
                ],
              ] as const
            ).map(
              ([mode, title, text]) =>
                html`<label class="flex items-start gap-3 cursor-pointer">
                  <input
                    type="radio"
                    name="omb-verify"
                    class="radio radio-sm radio-primary mt-0.5"
                    .checked=${d.verify_mode === mode}
                    @change=${() => (this.draft = { ...d, verify_mode: mode })}
                  />
                  <span
                    ><span class="font-medium">${title}</span
                    ><span class="block text-sm text-base-content/60">${text}</span></span
                  >
                </label>`,
            )}
          </div>
        </section>
        <section>
          <h4 class="font-medium">Project capture ranges</h4>
          <label class="flex items-start gap-3 cursor-pointer mt-2">
            <input
              type="checkbox"
              class="toggle toggle-primary mt-0.5"
              .checked=${d.allow_project_overlap ?? true}
              @change=${(e: Event) =>
                (this.draft = { ...d, allow_project_overlap: (e.target as HTMLInputElement).checked })}
            />
            <span>
              <span class="font-medium">Allow projects to have overlapping ranges</span>
              <span class="block text-sm text-base-content/60">
                When disabled, saving a project with an inclusive UTC capture range that overlaps another
                project is rejected.
              </span>
            </span>
          </label>
        </section>
        <section>
          <h4 class="font-medium">Full-card backup folder name</h4>
          <p class="text-sm text-base-content/60">
            Used by destinations with “Full-card backup folder” on. The name is written to the card the first
            time so later backups go to the same folder.
          </p>
          <omb-template-input
            .value=${d.backup_marker_template}
            placeholder="{date}_{project_name}"
            .vars=${previewVars(d, this.store.project)}
            @value-change=${(e: CustomEvent<string>) => (this.draft = { ...d, backup_marker_template: e.detail })}
          ></omb-template-input>
        </section>
      </div>
    `;
    const actions = html`
      <button class="btn btn-ghost" @click=${() => this.dismiss()}>Cancel</button>
      <button
        class="btn btn-primary"
        ?disabled=${!d.name.trim() || errors.length > 0}
        @click=${() => this.#save()}
      >
        Save
      </button>
    `;
    return html`<omb-modal
      size="lg"
      heading="Space settings"
      subheading=${d.name}
      icon=${d.icon}
      @close=${this.onClosed}
      .body=${body}
      .actions=${actions}
    ></omb-modal>`;
  }
}
