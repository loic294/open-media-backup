import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { Destination } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { newDestination } from "../../state/factories";
import { deviceById, mappingFor, nextPosition, spaceDestinations } from "../../state/selectors";
import { appDisplayName, configuredDestinationApp } from "../../utils/preview-apps";
import { previewVars, templateVars } from "../../utils/template";
import { destinationTaskName } from "../../utils/names";
import { ruleError } from "../form/rules-editor";
import { DialogBase } from "./dialog-base";
import "../form/device-field";
import "../form/rules-editor";
import "../form/template-input";
import "../form/remote-hash-field";

@customElement("omb-destination-dialog")
export class OmbDestinationDialog extends DialogBase<
  Extract<DialogRequest, { type: "destination-settings" }>
> {
  @state() private draft!: Destination;
  @state() private pendingAppPath: string | null | undefined;

  override connectedCallback(): void {
    super.connectedCallback();
    const { snapshot, space } = this.store;
    const existing = snapshot?.destinations.find((d) => d.id === this.request.destinationId);
    this.draft = existing
      ? structuredClone(existing)
      : newDestination(space!.id, "", nextPosition(spaceDestinations(snapshot!, space!.id)));
  }

  get #isNew() {
    return !this.request.destinationId;
  }

  async #save() {
    const saved = await this.store.save("destination", {
      ...this.draft,
      task_name: this.draft.task_name?.trim() ?? "",
    });
    if (!saved) return;
    if ((this.draft.kind ?? "folder") === "app" && this.pendingAppPath !== undefined) {
      await this.#saveLocalApp(this.draft.id, this.pendingAppPath);
    }
    this.dismiss();
  }

  async #pickApp() {
    const app = await this.store.backend.pickPreviewApp(this.store.snapshot?.computer.os ?? "");
    if (!app) return;
    this.pendingAppPath = app;
    this.draft = { ...this.draft, app_name: appDisplayName(app) };
    if (!this.#isNew) await this.#saveLocalApp(this.draft.id, app);
  }

  async #clearApp() {
    this.pendingAppPath = null;
    if (!this.#isNew) await this.#saveLocalApp(this.draft.id, null);
  }

  async #saveLocalApp(destinationId: string, appPath: string | null) {
    const current = this.store.snapshot?.settings.app_destinations ?? {};
    const app_destinations = { ...current };
    if (appPath?.trim()) app_destinations[destinationId] = appPath.trim();
    else delete app_destinations[destinationId];
    await this.store.saveSettings({ app_destinations });
  }

  #delete() {
    const name = destinationTaskName(
      this.draft,
      deviceById(this.store.snapshot!, this.draft.device_id),
      configuredDestinationApp(this.store.snapshot!.settings, this.draft.id),
    );
    this.store.open({
      type: "confirm",
      title: `Remove ${name}?`,
      message: "Its connections are removed too. Copied files stay on the device and remain in the catalog.",
      confirmLabel: "Remove destination",
      danger: true,
      onConfirm: async () => {
        await this.store.remove("destination", this.draft.id);
        this.dismiss();
      },
    });
  }

  #toggle(
    key: "subfolder_per_source" | "preserve_file_structure" | "counts_as_safe_copy" | "use_backup_marker",
    title: string,
    text: string,
  ) {
    return html`<label class="flex items-start gap-3 cursor-pointer">
      <input
        type="checkbox"
        class="toggle toggle-primary mt-0.5"
        .checked=${this.draft[key]}
        @change=${(e: Event) => (this.draft = { ...this.draft, [key]: (e.target as HTMLInputElement).checked })}
      />
      <span
        ><span class="font-medium">${title}</span
        ><span class="block text-sm text-base-content/60">${text}</span></span
      >
    </label>`;
  }

  override render() {
    const { snapshot, space } = this.store;
    if (!snapshot || !space) return nothing;
    const project = snapshot.projects.find((item) => item.space_id === space.id && !item.archived) ?? null;
    const d = this.draft;
    const isApp = (d.kind ?? "folder") === "app";
    const localAppPath =
      this.pendingAppPath !== undefined
        ? this.pendingAppPath
        : configuredDestinationApp(snapshot.settings, d.id);
    const device = deviceById(snapshot, d.device_id);
    const set = (patch: Partial<Destination>) => (this.draft = { ...d, ...patch });
    const vars = previewVars(space, project);
    const projectVariableNames = [
      ...new Set([
        ...space.variables.map((variable) => variable.name),
        ...snapshot.projects
          .filter((item) => item.space_id === space.id && !item.archived)
          .flatMap((item) => Object.keys(item.values)),
      ]),
    ];
    const knownTemplateVariables = new Set(["project", "project_name", ...projectVariableNames]);
    if (d.use_backup_marker)
      vars.backup_folder = `${vars.date}_${vars.project_name ?? "project"} (from card marker)`;
    const unknownVars = isApp
      ? []
      : templateVars(d.path_template).filter((v) => !(v in vars) && !knownTemplateVariables.has(v));
    const invalid =
      (!isApp && !!d.remote_hash?.enabled && (!d.remote_hash.server_id || !d.remote_hash.root)) ||
      d.rules.some((r) => ruleError(r)) ||
      unknownVars.length > 0 ||
      (isApp ? !d.app_name?.trim() && !localAppPath?.trim() : !device);
    const body = html`
      <div class="flex flex-col gap-5">
        <fieldset class="fieldset">
          <legend class="fieldset-legend">Destination task name</legend>
          <input
            aria-label="Destination task name"
            class="input w-full"
            .value=${d.task_name ?? ""}
            placeholder=${destinationTaskName({ ...d, task_name: "" }, device, localAppPath)}
            @input=${(e: Event) => set({ task_name: (e.target as HTMLInputElement).value })}
          />
          <p class="label whitespace-normal">
            Only labels this card and its transfers. Leave blank to use the device or application name.
          </p>
        </fieldset>
        <section>
          <h4 class="font-medium mb-2">Type</h4>
          <div class="inline-flex rounded-field border border-base-300 bg-base-200 p-1">
            <button
              class="btn btn-sm ${isApp ? "btn-ghost" : "btn-primary"}"
              @click=${() => set({ kind: "folder" })}
            >
              Folder
            </button>
            <button
              class="btn btn-sm ${isApp ? "btn-primary" : "btn-ghost"}"
              @click=${() =>
                set({
                  kind: "app",
                  device_id: "",
                  path_template: "",
                  subfolder_per_source: false,
                  counts_as_safe_copy: false,
                  use_backup_marker: false,
                  remote_hash: null,
                })}
            >
              App
            </button>
          </div>
        </section>
        ${
          isApp
            ? html`
                <section>
                  <h4 class="font-medium mb-2">Application</h4>
                  <div class="flex items-center gap-3 rounded-box border border-base-300 bg-base-100 p-3">
                    <span
                      class="grid size-10 place-items-center rounded-box bg-base-200 text-base-content/70"
                    >
                      <omb-icon name="external-link" class="size-5"></omb-icon>
                    </span>
                    <div class="min-w-0 flex-1">
                      <div class="font-medium truncate">
                        ${d.app_name || (localAppPath ? appDisplayName(localAppPath) : "No application selected")}
                      </div>
                      <div class="text-sm text-base-content/60 truncate">
                        ${localAppPath || "Not set on this computer"}
                      </div>
                    </div>
                    <button class="btn btn-sm" @click=${() => this.#pickApp()}>
                      ${localAppPath ? "Change…" : "Choose…"}
                    </button>
                    ${
                      localAppPath
                        ? html`<button class="btn btn-ghost btn-sm" @click=${() => this.#clearApp()}>
                            Clear
                          </button>`
                        : nothing
                    }
                  </div>
                  <p class="mt-2 text-sm text-base-content/60">
                    App destinations are manual only. They open matching files in the selected app and never
                    run during automatic transfers.
                  </p>
                  ${this.#toggle(
                    "counts_as_safe_copy",
                    "Counts as a safe copy",
                    "Confirmed imports here count one-for-one toward copies required before wiping a card.",
                  )}
                </section>
                <section>
                  <h4 class="font-medium mb-2">File rules</h4>
                  <omb-rules-editor
                    .rules=${d.rules}
                    .variables=${projectVariableNames}
                    @rules-change=${(e: CustomEvent) => set({ rules: e.detail })}
                  ></omb-rules-editor>
                </section>
              `
            : html`
                <section>
                  <h4 class="font-medium mb-2">Device</h4>
                  <omb-device-field
                    .deviceId=${d.device_id || null}
                    defaultRole="final"
                    @device-change=${(e: CustomEvent<string>) => set({ device_id: e.detail })}
                  ></omb-device-field>
                </section>
                ${
                  device
                    ? html`
                        <omb-template-input
                          label="Destination folder"
                          .prefix=${mappingFor(snapshot, device.id)?.root_path ?? device.name}
                          .value=${d.path_template}
                          .vars=${vars}
                          hint="Use {variables} from the space; values come from the matching active project."
                          @value-change=${(e: CustomEvent<string>) => set({ path_template: e.detail })}
                        ></omb-template-input>
                        ${
                          unknownVars.includes("backup_folder")
                            ? html`<p class="text-sm text-warning -mt-2">
                                {backup_folder} comes from the card marker: turn on “Full-card backup folder”
                                below.
                              </p>`
                            : nothing
                        }
                        <div class="flex flex-col gap-3">
                          ${this.#toggle("subfolder_per_source", "Subfolder per source", "Copies go into a folder named after the source's device name for backup, or its physical device name when blank. Task names never affect folders.")}
                          ${this.#toggle(
                            "preserve_file_structure",
                            "Preserve original file structure",
                            "When off, files go directly into the destination folder instead of recreating their source subfolders. Destination and per-source folders still apply.",
                          )}
                          ${this.#toggle(
                            "use_backup_marker",
                            "Full-card backup folder",
                            `Every file of a card goes to the same folder. Its name is stored in a small file on the card and used as {backup_folder}; when missing it is created from “${space.backup_marker_template}”.`,
                          )}
                          ${
                            device.role === "final"
                              ? this.#toggle(
                                  "counts_as_safe_copy",
                                  "Counts as a safe copy",
                                  "Verified copies here count one-for-one toward the copies required before wiping a card.",
                                )
                              : device.role === "temporary"
                                ? this.#toggle(
                                    "counts_as_safe_copy",
                                    "Counts as a temporary safe copy",
                                    (space.temporary_copies_per_final ?? 0) > 0
                                      ? `${space.temporary_copies_per_final} verified temporary ${space.temporary_copies_per_final === 1 ? "copy counts" : "copies count"} as one final copy for this space.`
                                      : "Enable temporary safe copies in space settings before temporary destinations contribute.",
                                  )
                                : html`<p class="text-sm text-base-content/60">
                                    Original devices never count as safe-copy destinations.
                                  </p>`
                          }
                        </div>
                        <omb-remote-hash-field
                          .mapping=${d.remote_hash ?? null}
                          .destinationId=${this.#isNew ? "" : d.id}
                          .deviceId=${d.device_id}
                          @remote-hash-change=${(e: CustomEvent) => set({ remote_hash: e.detail })}
                        ></omb-remote-hash-field>
                        <section>
                          <h4 class="font-medium mb-2">File rules</h4>
                          <omb-rules-editor
                            .rules=${d.rules}
                            .variables=${projectVariableNames}
                            @rules-change=${(e: CustomEvent) => set({ rules: e.detail })}
                          ></omb-rules-editor>
                        </section>
                      `
                    : nothing
                }
              `
        }
      </div>
    `;
    const actions = html`
      ${this.#isNew ? nothing : html`<button class="btn btn-ghost text-error mr-auto" @click=${() => this.#delete()}><omb-icon name="trash"></omb-icon>Remove</button>`}
      <button class="btn btn-ghost" @click=${() => this.dismiss()}>Cancel</button>
      <button class="btn btn-primary" ?disabled=${invalid} @click=${() => this.#save()}>
        ${this.#isNew ? "Add destination" : "Save"}
      </button>
    `;
    return html`<omb-modal
      size="lg"
      heading=${
        this.#isNew ? "Add destination" : `Destination · ${destinationTaskName(d, device, localAppPath)}`
      }
      subheading=${
        isApp
          ? "Open matching files in an application for a manual import."
          : "Where files are copied and verified: an SSD, NAS or archive drive."
      }
      icon=${isApp ? "external-link" : "log-out"}
      @close=${this.onClosed}
      .body=${body}
      .actions=${actions}
    ></omb-modal>`;
  }
}
