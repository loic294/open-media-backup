import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import { OmbElement } from "../ui/omb-element";

/** Opens project management actions while keeping active project state internal. */
@customElement("omb-project-picker")
export class OmbProjectPicker extends OmbElement {
  override render() {
    const { space } = this.store;
    if (!space) return nothing;
    return html`<div class="flex items-center justify-center gap-2">
      <button
        class="btn btn-sm btn-primary btn-outline gap-2 px-5"
        aria-label="Open projects"
        @click=${() => this.store.openProjectsPage()}
      >
        <omb-icon name="folder"></omb-icon>Projects
      </button>
      <button
        class="btn btn-sm btn-primary gap-2 px-5"
        aria-label="Create new project"
        @click=${() => this.store.open({ type: "project", projectId: null })}
      >
        <omb-icon name="plus"></omb-icon>New project
      </button>
    </div>`;
  }
}
