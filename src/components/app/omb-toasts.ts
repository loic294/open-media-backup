import { html } from "lit";
import { customElement } from "lit/decorators.js";
import { OmbElement } from "../ui/omb-element";

const ALERT = { info: "alert-info", success: "alert-success", warning: "alert-warning", error: "alert-error" } as const;
const ICON = { info: "info", success: "circle-check", warning: "triangle-alert", error: "circle-alert" } as const;

@customElement("omb-toasts")
export class OmbToasts extends OmbElement {
  override render() {
    return html`
      <div class="toast toast-end toast-bottom mb-20 z-50">
        ${this.store.toasts.map(
          (t) => html`<div role="alert" class="alert ${ALERT[t.kind]} alert-soft shadow-lg max-w-sm">
            <omb-icon name=${ICON[t.kind]}></omb-icon><span class="text-sm">${t.message}</span>
          </div>`,
        )}
      </div>
    `;
  }
}
