import { html, nothing, type TemplateResult } from "lit";
import { customElement, property, query } from "lit/decorators.js";
import { OmbPureElement } from "./omb-element";
import "./omb-icon";

/**
 * daisyUI modal shell. Opens itself on connect; emits "close" when dismissed.
 * Content is provided via the `body` / `actions` template properties (light DOM has no slots).
 */
@customElement("omb-modal")
export class OmbModal extends OmbPureElement {
  @property() heading = "";
  @property() subheading = "";
  @property() icon = "";
  @property() size: "sm" | "md" | "lg" | "xl" = "md";
  @property({ attribute: false }) body: TemplateResult | typeof nothing = nothing;
  @property({ attribute: false }) actions: TemplateResult | typeof nothing = nothing;
  @query("dialog") private dialog!: HTMLDialogElement;

  override firstUpdated(): void {
    this.dialog.showModal();
  }

  close(): void {
    this.dialog.close();
  }

  #onClose = () => this.dispatchEvent(new CustomEvent("close"));

  override render() {
    const width = { sm: "max-w-md", md: "max-w-xl", lg: "max-w-3xl", xl: "max-w-6xl" }[this.size];
    return html`
      <dialog class="modal" @close=${this.#onClose}>
        <div class="modal-box ${width} w-full p-0 flex flex-col max-h-[88vh]">
          <header class="flex items-start gap-3 px-6 pt-5 pb-4 border-b border-base-300">
            ${this.icon
              ? html`<span class="grid place-items-center size-9 rounded-box bg-primary/15 text-primary"><omb-icon name=${this.icon} class="size-5"></omb-icon></span>`
              : nothing}
            <div class="flex-1 min-w-0">
              <h3 class="font-semibold text-lg leading-tight">${this.heading}</h3>
              ${this.subheading ? html`<p class="text-sm text-base-content/60 mt-0.5">${this.subheading}</p>` : nothing}
            </div>
            <form method="dialog"><button class="btn btn-ghost btn-sm btn-square" aria-label="Close"><omb-icon name="x"></omb-icon></button></form>
          </header>
          <div class="px-6 py-5 overflow-y-auto flex-1">${this.body}</div>
          ${this.actions !== nothing ? html`<footer class="flex items-center justify-end gap-2 px-6 py-4 border-t border-base-300">${this.actions}</footer>` : nothing}
        </div>
        <form method="dialog" class="modal-backdrop"><button>close</button></form>
      </dialog>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "omb-modal": OmbModal;
  }
}
