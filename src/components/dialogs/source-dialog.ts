import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { Source } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { newSource } from "../../state/factories";
import { deviceById, mappingFor, nextPosition, spaceSources } from "../../state/selectors";
import { previewVars } from "../../utils/template";
import { DialogBase } from "./dialog-base";
import "../form/device-field";
import "../form/template-input";

@customElement("omb-source-dialog")
export class OmbSourceDialog extends DialogBase<Extract<DialogRequest, { type: "source-settings" }>> {
  @state() private draft!: Source;

  override connectedCallback(): void {
    super.connectedCallback();
    const { snapshot, space } = this.store;
    const existing = snapshot?.sources.find((s) => s.id === this.request.sourceId);
    this.draft = existing
      ? structuredClone(existing)
      : newSource(space!.id, "", nextPosition(spaceSources(snapshot!, space!.id)));
  }

  get #isNew() {
    return !this.request.sourceId;
  }

  async #save() {
    const previous = this.store.snapshot;
    await this.store.save("source", this.draft);
    if (this.store.snapshot === previous) return;
    this.dismiss();
  }

  #delete() {
    const name = deviceById(this.store.snapshot!, this.draft.device_id)?.name ?? "this source";
    this.store.open({
      type: "confirm",
      title: `Remove ${name}?`,
      message: "Its connections are removed too. Files and the backup history stay untouched.",
      confirmLabel: "Remove source",
      danger: true,
      onConfirm: async () => {
        await this.store.remove("source", this.draft.id);
        this.dismiss();
      },
    });
  }

  override render() {
    const { snapshot, space, project } = this.store;
    if (!snapshot || !space) return nothing;
    const d = this.draft;
    const device = deviceById(snapshot, d.device_id);
    const set = (patch: Partial<Source>) => (this.draft = { ...d, ...patch });
    const projects = snapshot.projects.filter((p) => p.space_id === space.id);
    const scope = d.project_scope ?? { mode: "all" as const };
    const setScopeMode = (mode: "all" | "selected" | "none") => {
      if (mode === "selected") {
        const projectIds = scope.mode === "selected" ? scope.project_ids : [];
        set({ project_scope: { mode, project_ids: projectIds } });
      } else {
        set({ project_scope: { mode } });
      }
    };
    const body = html`
      <div class="flex flex-col gap-4">
        <section>
          <h4 class="font-medium mb-2">Device</h4>
          <omb-device-field
            .deviceId=${d.device_id || null}
            .mountPath=${this.request.mountPath ?? ""}
            defaultRole="original"
            @device-change=${(e: CustomEvent<string>) => set({ device_id: e.detail })}
          ></omb-device-field>
        </section>
        ${
          device
            ? html`
                <omb-template-input
                  label="Folder on the device"
                  placeholder="Leave empty to back up the whole device, e.g. DCIM"
                  .prefix=${mappingFor(snapshot, device.id)?.root_path ?? device.name}
                  .value=${d.path_template}
                  .vars=${previewVars(space, project, device.name)}
                  @value-change=${(e: CustomEvent<string>) => set({ path_template: e.detail })}
                ></omb-template-input>
                <label class="flex items-start gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    class="toggle toggle-primary mt-0.5"
                    .checked=${d.offer_wipe}
                    @change=${(e: Event) => set({ offer_wipe: (e.target as HTMLInputElement).checked })}
                  />
                  <span>
                    <span class="font-medium">Offer to wipe once safe</span>
                    <span class="block text-sm text-base-content/60">
                      After every file is verified on ${project?.final_copies_required ?? 2} final
                      destinations, a “Wipe card” button appears. Final devices are never wiped.
                    </span>
                  </span>
                </label>
              `
            : nothing
        }
        <section class="flex flex-col gap-2">
          <div>
            <h4 class="font-medium">Projects matched by this source</h4>
            <p class="text-sm text-base-content/60">Choose which projects receive media from this source.</p>
          </div>
          <div class="flex flex-col gap-2">
            ${(
              [
                ["all", "Match all projects"],
                ["selected", "Match selected projects"],
                ["none", "Match no projects"],
              ] as const
            ).map(
              ([mode, label]) =>
                html`<label class="flex items-center gap-2 cursor-pointer">
                  <input
                    type="radio"
                    name="omb-source-project-scope"
                    class="radio radio-sm radio-primary"
                    .checked=${scope.mode === mode}
                    @change=${() => setScopeMode(mode)}
                  />
                  <span>${label}</span>
                </label>`,
            )}
          </div>
          ${
            scope.mode === "selected"
              ? html`<div class="rounded-box border border-base-300 p-3 flex flex-col gap-2">
                  ${
                    projects.length
                      ? projects.map(
                          (project) =>
                            html`<label class="flex items-center gap-2 cursor-pointer">
                              <input
                                type="checkbox"
                                class="checkbox checkbox-sm"
                                .checked=${scope.project_ids.includes(project.id)}
                                @change=${(e: Event) => {
                                  const checked = (e.target as HTMLInputElement).checked;
                                  const projectIds = checked
                                    ? [...new Set([...scope.project_ids, project.id])]
                                    : scope.project_ids.filter((id) => id !== project.id);
                                  set({ project_scope: { mode: "selected", project_ids: projectIds } });
                                }}
                              />
                              <span>${project.name}</span>
                            </label>`,
                        )
                      : html`<p class="text-sm text-base-content/60">This space has no projects yet.</p>`
                  }
                  ${
                    scope.project_ids.length === 0
                      ? html`<p class="text-xs text-warning">
                          No projects selected; this source will not match any project.
                        </p>`
                      : nothing
                  }
                </div>`
              : nothing
          }
        </section>
      </div>
    `;
    const actions = html`
      ${this.#isNew ? nothing : html`<button class="btn btn-ghost text-error mr-auto" @click=${() => this.#delete()}><omb-icon name="trash"></omb-icon>Remove</button>`}
      <button class="btn btn-ghost" @click=${() => this.dismiss()}>Cancel</button>
      <button class="btn btn-primary" ?disabled=${!d.device_id} @click=${() => this.#save()}>
        ${this.#isNew ? "Add source" : "Save"}
      </button>
    `;
    return html`<omb-modal
      heading=${this.#isNew ? "Add source" : `Source · ${device?.name ?? ""}`}
      subheading="Where media comes from: a memory card, camera, drone or any folder."
      icon="log-in"
      @close=${this.onClosed}
      .body=${body}
      .actions=${actions}
    ></omb-modal>`;
  }
}
