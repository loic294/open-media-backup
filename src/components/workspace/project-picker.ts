import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import { OmbElement } from "../ui/omb-element";

/** Shows the active project and links to the dedicated project management page. */
@customElement("omb-project-picker")
export class OmbProjectPicker extends OmbElement {
  override render() {
    const { project, space } = this.store;
    if (!space) return nothing;
    return html`<div class="flex items-center gap-3">
      <span class="text-sm tracking-wide text-base-content/60 uppercase">Project</span>
      <span class="flex min-w-0 items-center gap-2" aria-live="polite">
        ${
          project
            ? html`<span
                class="size-3 shrink-0 rounded-full border border-base-content/20"
                style=${`background-color:${project.color ?? "var(--color-primary)"}`}
              ></span>`
            : nothing
        }
        <span class="font-semibold truncate">${project?.name ?? "No project selected"}</span>
      </span>
      <button class="btn btn-sm gap-2" @click=${() => this.store.openProjectsPage()}>
        <omb-icon name="folder-open"></omb-icon>Projects
      </button>
    </div>`;
  }
}
