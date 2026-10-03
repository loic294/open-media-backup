import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { Destination } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { newDestination } from "../../state/factories";
import { deviceById, mappingFor, nextPosition, spaceDestinations } from "../../state/selectors";
import { previewVars, templateVars } from "../../utils/template";
import { ruleError } from "../form/rules-editor";
import { DialogBase } from "./dialog-base";
import "../form/device-field";
import "../form/rules-editor";
import "../form/template-input";

@customElement("omb-destination-dialog")
export class OmbDestinationDialog extends DialogBase<Extract<DialogRequest, { type: "destination-settings" }>> {
  @state() private draft!: Destination;

  override connectedCallback(): void {
    super.connectedCallback();
    const { snapshot, space } = this.store;
    const existing = snapshot?.destinations.find((d) => d.id === this.request.destinationId);
    this.draft = existing ? structuredClone(existing) : newDestination(space!.id, "", nextPosition(spaceDestinations(snapshot!, space!.id)));
  }

  get #isNew() {
    return !this.request.destinationId;
  }

  async #save() {
    await this.store.save("destination", this.draft);
    this.dismiss();
  }

  #delete() {
    const name = deviceById(this.store.snapshot!, this.draft.device_id)?.name ?? "this destination";
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

  #toggle(key: "subfolder_per_source" | "counts_as_safe_copy" | "use_backup_marker", title: string, text: string) {
    return html`<label class="flex items-start gap-3 cursor-pointer">
      <input type="checkbox" class="toggle toggle-primary mt-0.5" .checked=${this.draft[key]} @change=${(e: Event) => (this.draft = { ...this.draft, [key]: (e.target as HTMLInputElement).checked })} />
      <span><span class="font-medium">${title}</span><span class="block text-sm text-base-content/60">${text}</span></span>
    </label>`;
  }

  override render() {
    const { snapshot, space, project } = this.store;
    if (!snapshot || !space) return nothing;
    const d = this.draft;
    const device = deviceById(snapshot, d.device_id);
    const set = (patch: Partial<Destination>) => (this.draft = { ...d, ...patch });
    const vars = previewVars(space, project);
    if (d.use_backup_marker) vars.backup_folder = `${vars.date}_${vars.project_name ?? "project"} (from card marker)`;
    const unknownVars = templateVars(d.path_template).filter((v) => !(v in vars));
    const invalid = d.rules.some((r) => ruleError(r)) || unknownVars.length > 0;
    const body = html`
      <div class="flex flex-col gap-5">
        <section>
          <h4 class="font-medium mb-2">Device</h4>
          <omb-device-field .deviceId=${d.device_id || null} defaultRole="final" @device-change=${(e: CustomEvent<string>) => set({ device_id: e.detail })}></omb-device-field>
        </section>
        ${device
          ? html`
              <omb-template-input
                label="Destination folder"
                .prefix=${mappingFor(snapshot, device.id)?.root_path ?? device.name}
                .value=${d.path_template}
                .vars=${vars}
                hint="Use {variables} from the space; values come from the selected project."
                @value-change=${(e: CustomEvent<string>) => set({ path_template: e.detail })}
              ></omb-template-input>
              ${unknownVars.includes("backup_folder")
                ? html`<p class="text-sm text-warning -mt-2">{backup_folder} comes from the card marker: turn on “Full-card backup folder” below.</p>`
                : nothing}
              <div class="flex flex-col gap-3">
                ${this.#toggle("subfolder_per_source", "Subfolder per source", "Copies go into a folder named after the source device, e.g. …/Camera A · Card 1/.")}
                ${this.#toggle(
                  "use_backup_marker",
                  "Full-card backup folder",
                  `Every file of a card goes to the same folder. Its name is stored in a small file on the card and used as {backup_folder}; when missing it is created from “${space.backup_marker_template}”.`,
                )}
                ${device.role === "final"
                  ? this.#toggle("counts_as_safe_copy", "Counts as a safe copy", "Verified copies here count toward the copies required before wiping a card.")
                  : html`<p class="text-sm text-base-content/60">Only devices with the <b>final</b> role count as safe copies.</p>`}
              </div>
              <section>
                <h4 class="font-medium mb-2">File rules</h4>
                <omb-rules-editor .rules=${d.rules} @rules-change=${(e: CustomEvent) => set({ rules: e.detail })}></omb-rules-editor>
              </section>
            `
          : nothing}
      </div>
    `;
    const actions = html`
      ${this.#isNew ? nothing : html`<button class="btn btn-ghost text-error mr-auto" @click=${() => this.#delete()}><omb-icon name="trash"></omb-icon>Remove</button>`}
      <button class="btn btn-ghost" @click=${() => this.dismiss()}>Cancel</button>
      <button class="btn btn-primary" ?disabled=${!d.device_id || invalid} @click=${() => this.#save()}>${this.#isNew ? "Add destination" : "Save"}</button>
    `;
    return html`<omb-modal
      size="lg"
      heading=${this.#isNew ? "Add destination" : `Destination · ${device?.name ?? ""}`}
      subheading="Where files are copied and verified: an SSD, NAS or archive drive."
      icon="log-out"
      @close=${this.onClosed}
      .body=${body}
      .actions=${actions}
    ></omb-modal>`;
  }
}
