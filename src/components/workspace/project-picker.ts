import { html, nothing } from "lit";
import { customElement, query } from "lit/decorators.js";
import type { Project } from "../../api/types";
import { spaceProjects } from "../../state/selectors";
import { OmbElement } from "../ui/omb-element";

/** Centered PROJECT dropdown with a "new project" button. Uses the popover API. */
@customElement("omb-project-picker")
export class OmbProjectPicker extends OmbElement {
  @query("[popover]") private menu!: HTMLElement;

  #pick(p: Project) {
    this.menu.hidePopover();
    void this.store.selectProject(p.id);
  }

  #item(p: Project, active: boolean) {
    return html`<li>
      <button class="flex justify-between ${active ? "menu-active" : ""}" @click=${() => this.#pick(p)}>
        <span class="truncate ${p.archived ? "opacity-60" : ""}">${p.name}</span>
        ${p.archived ? html`<span class="badge badge-ghost badge-xs">archived</span>` : nothing}
      </button>
    </li>`;
  }

  override render() {
    const { snapshot, space, project } = this.store;
    if (!snapshot || !space) return nothing;
    const projects = spaceProjects(snapshot, space.id);
    const vars = project ? Object.values(project.values).filter((v) => v.trim()).length : 0;
    return html`
      <div class="flex items-center gap-3">
        <span class="text-sm tracking-wide text-base-content/60 uppercase">Project</span>
        <button class="btn h-11 min-w-72 justify-between gap-3 bg-base-100 border-base-300 font-normal" popovertarget="omb-project-menu" style="anchor-name:--omb-project">
          <span class="flex items-center gap-3 min-w-0">
            <omb-icon name="folder-open" class="text-primary"></omb-icon>
            <span class="font-semibold truncate">${project?.name ?? "No project"}</span>
          </span>
          <span class="flex items-center gap-2">
            ${project ? html`<span class="badge badge-primary badge-soft badge-sm">${vars} vars</span>` : nothing}
            <omb-icon name="chevrons-up-down" class="opacity-60"></omb-icon>
          </span>
        </button>
        <ul popover id="omb-project-menu" class="dropdown menu w-72 rounded-box bg-base-100 shadow-lg border border-base-300 p-2" style="position-anchor:--omb-project">
          ${projects.length === 0 ? html`<li class="menu-disabled"><span>No projects yet</span></li>` : nothing}
          ${projects.map((p) => this.#item(p, p.id === project?.id))}
          <li class="mt-1 border-t border-base-300 pt-1">
            <button @click=${() => (this.menu.hidePopover(), this.store.open({ type: "project", projectId: null }))}><omb-icon name="plus"></omb-icon>New project…</button>
          </li>
          ${project
            ? html`<li>
                <button @click=${() => (this.menu.hidePopover(), this.store.open({ type: "project", projectId: project.id }))}>
                  <omb-icon name="pencil"></omb-icon>Edit “${project.name}”…
                </button>
              </li>`
            : nothing}
        </ul>
        <button class="btn btn-square h-11 w-11 bg-base-100 border-base-300" title="New project" @click=${() => this.store.open({ type: "project", projectId: null })}>
          <omb-icon name="folder-plus" class="size-5"></omb-icon>
        </button>
      </div>
    `;
  }
}
