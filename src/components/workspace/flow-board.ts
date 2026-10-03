import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import { spaceDestinations, spaceSources } from "../../state/selectors";
import { OmbElement } from "../ui/omb-element";
import "./destination-card";
import "./flow-canvas";
import "./new-volumes";
import "./source-card";

/** Sources (left), connections (middle) and destinations (right). */
@customElement("omb-flow-board")
export class OmbFlowBoard extends OmbElement {
  #heading(icon: string, label: string, count: number, add: () => void, addLabel: string) {
    return html`
      <div class="flex items-center justify-between gap-3">
        <h2
          class="flex items-center gap-2 text-sm font-semibold tracking-widest uppercase text-base-content/60"
        >
          <omb-icon name=${icon}></omb-icon>${label}<span class="badge badge-sm badge-neutral">${count}</span>
        </h2>
        <button class="btn btn-sm btn-outline border-base-300 gap-1.5 font-normal" @click=${add}>
          <omb-icon name="plus"></omb-icon>${addLabel}
        </button>
      </div>
    `;
  }

  override render() {
    const { snapshot, space } = this.store;
    if (!snapshot || !space) return nothing;
    const sources = spaceSources(snapshot, space.id);
    const destinations = spaceDestinations(snapshot, space.id);
    return html`
      <div
        class="relative grid grid-cols-[minmax(300px,30rem)_minmax(8rem,1fr)_minmax(26rem,48rem)] gap-y-5 h-full content-start"
        data-flow-board
      >
        <div class="col-start-1">
          ${this.#heading("log-in", "Sources", sources.length, () => this.store.open({ type: "source-settings", sourceId: null }), "Add source")}
        </div>
        <div class="col-start-3">
          ${this.#heading("log-out", "Destinations", destinations.length, () => this.store.open({ type: "destination-settings", destinationId: null }), "Add destination")}
        </div>
        <div class="col-start-1 flex flex-col gap-5">
          ${sources.map((s) => html`<omb-source-card data-omb-block .source=${s}></omb-source-card>`)}
          <omb-new-volumes data-omb-block></omb-new-volumes>
        </div>
        <div class="col-start-3 flex flex-col gap-5">
          ${destinations.map((d) => html`<omb-destination-card data-omb-block .destination=${d}></omb-destination-card>`)}
          ${
            destinations.length === 0
              ? html`<button
                  class="card border border-dashed border-base-300 p-5 text-sm text-base-content/60 hover:border-primary"
                  @click=${() => this.store.open({ type: "destination-settings", destinationId: null })}
                >
                  Add an SSD, NAS or folder where files should be copied
                </button>`
              : nothing
          }
        </div>
        <omb-flow-canvas class="absolute inset-0 pointer-events-none"></omb-flow-canvas>
      </div>
    `;
  }
}
