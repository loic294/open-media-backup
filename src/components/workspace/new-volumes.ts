import { html } from "lit";
import { customElement } from "lit/decorators.js";
import { formatBytes } from "../../utils/format";
import { OmbElement } from "../ui/omb-element";

/** Hint card plus one-click "add as source" for newly inserted, unknown volumes. */
@customElement("omb-new-volumes")
export class OmbNewVolumes extends OmbElement {
  override render() {
    const unknown = this.store.volumes.filter((v) => !v.device_id && v.removable);
    return html`
      <div class="flex flex-col gap-2">
        ${unknown.map(
          (v) => html`
            <div class="flex items-center gap-3 rounded-box border border-primary/40 bg-primary/5 px-4 py-3">
              <omb-icon name="card-sim" class="text-primary"></omb-icon>
              <span class="flex-1 text-sm"><b>${v.name}</b> inserted · ${formatBytes(v.total_bytes)}</span>
              <button class="btn btn-xs btn-primary" @click=${() => this.store.open({ type: "source-settings", sourceId: null, mountPath: v.mount_path })}>Add as source</button>
            </div>
          `,
        )}
        <button
          class="flex items-center gap-3 rounded-box border border-dashed border-base-300 px-4 py-4 text-sm text-base-content/60 hover:border-primary hover:text-base-content transition-colors"
          @click=${() => this.store.open({ type: "source-settings", sourceId: null })}
        >
          <omb-icon name="unplug" class="size-5"></omb-icon>Insert a memory card or choose a folder to add a source
        </button>
      </div>
    `;
  }
}
