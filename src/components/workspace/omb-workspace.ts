import { html } from "lit";
import { customElement } from "lit/decorators.js";
import { OmbElement } from "../ui/omb-element";
import "./space-header";
import "./flow-board";

@customElement("omb-workspace")
export class OmbWorkspace extends OmbElement {
  override render() {
    const space = this.store.space;
    if (!space) {
      return html`
        <div class="h-full grid place-items-center text-center">
          <div class="max-w-sm space-y-3">
            <omb-icon name="folder-plus" class="size-10 text-primary"></omb-icon>
            <h2 class="text-lg font-semibold">Create your first space</h2>
            <p class="text-sm text-base-content/60">
              A space groups the sources and destinations you use together, like “Travel” or “Home”.
            </p>
            <p class="text-sm text-base-content/60">Use the <b>+</b> button at the top to add one.</p>
          </div>
        </div>
      `;
    }
    return html`
      <div class="h-full overflow-auto px-6 py-5 flex flex-col gap-6">
        <omb-space-header></omb-space-header>
        ${
          this.store.statusError
            ? html`
                <div role="alert" class="alert alert-error alert-soft sm:alert-horizontal">
                  <omb-icon name="circle-alert"></omb-icon>
                  <span>Could not load transfer status: ${this.store.statusError}</span>
                  <button class="btn btn-sm" @click=${() => this.store.retryStatus()}>Retry</button>
                </div>
              `
            : ""
        }
        <omb-flow-board class="flex-1"></omb-flow-board>
      </div>
    `;
  }
}
