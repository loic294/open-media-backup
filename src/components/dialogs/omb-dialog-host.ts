import { html, type TemplateResult } from "lit";
import { customElement } from "lit/decorators.js";
import type { DialogRequest, DialogType } from "../../state/dialogs";
import { OmbElement } from "../ui/omb-element";
import "./app-settings-dialog";
import "./confirm-dialog";
import "./destination-dialog";
import "./device-dialog";
import "./preview-dialog";
import "./media-browser-dialog";
import "./project-dialog";
import "./source-dialog";
import "./space-dialog";
import "./sync-dialog";
import "./update-dialog";
import "./wipe-dialog";
import "../workspace/projects-page";

type Renderer<T extends DialogType> = (r: Extract<DialogRequest, { type: T }>) => TemplateResult;

const RENDERERS: { [T in DialogType]: Renderer<T> } = {
  confirm: (r) => html`<omb-confirm-dialog .request=${r}></omb-confirm-dialog>`,
  "device-sync": (r) => html`<omb-sync-dialog .request=${r}></omb-sync-dialog>`,
  "app-settings": (r) => html`<omb-app-settings-dialog .request=${r}></omb-app-settings-dialog>`,
  "device-settings": (r) => html`<omb-device-dialog .request=${r}></omb-device-dialog>`,
  update: (r) => html`<omb-update-dialog .request=${r}></omb-update-dialog>`,
  projects: (r) => html`<omb-projects-page .request=${r}></omb-projects-page>`,
  project: (r) => html`<omb-project-dialog .request=${r}></omb-project-dialog>`,
  "space-settings": (r) => html`<omb-space-dialog .request=${r}></omb-space-dialog>`,
  "source-settings": (r) => html`<omb-source-dialog .request=${r}></omb-source-dialog>`,
  "destination-settings": (r) => html`<omb-destination-dialog .request=${r}></omb-destination-dialog>`,
  "wipe-card": (r) => html`<omb-wipe-dialog .request=${r}></omb-wipe-dialog>`,
  preview: (r) => html`<omb-preview-dialog .request=${r}></omb-preview-dialog>`,
  "media-browser": (r) => html`<omb-media-browser-dialog .request=${r}></omb-media-browser-dialog>`,
};

/** Renders every open dialog (stacked) from store.dialogs. */
@customElement("omb-dialog-host")
export class OmbDialogHost extends OmbElement {
  override render() {
    return this.store.dialogs.map((d) => (RENDERERS[d.type] as Renderer<DialogType>)(d as never));
  }
}
