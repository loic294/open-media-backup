import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import { projectTotals } from "../../state/derived";
import { spaceFlows } from "../../state/selectors";
import { formatCount, plural } from "../../utils/format";
import { closeDropdown } from "../ui/dropdown";
import { OmbElement } from "../ui/omb-element";
import "./project-picker";

@customElement("omb-space-header")
export class OmbSpaceHeader extends OmbElement {
  #run(e: Event, action: () => void) {
    closeDropdown(e.currentTarget as Element);
    action();
  }

  #delete() {
    const space = this.store.space;
    if (!space) return;
    this.store.open({
      type: "confirm",
      title: `Delete “${space.name}”?`,
      message:
        "Its projects, sources, destinations and connections are removed on every synced device. Files and the backup catalog are kept.",
      confirmLabel: "Delete space",
      danger: true,
      onConfirm: () => this.store.remove("space", space.id),
    });
  }

  override render() {
    const { space, snapshot, status } = this.store;
    if (!space || !snapshot) return nothing;
    const totals = projectTotals(status, spaceFlows(snapshot, space.id));
    const open = (focus: "name" | "variables") =>
      this.store.open({ type: "space-settings", spaceId: space.id, focus });
    return html`
      <div class="grid grid-cols-[1fr_auto_1fr] items-center gap-4">
        <div class="flex items-center gap-1 min-w-0">
          <h1 class="text-xl font-bold mr-1 truncate">${space.name}</h1>
          <details class="dropdown">
            <summary class="btn btn-ghost btn-sm btn-square" title="Space options" aria-label="Space options">
              <omb-icon name="ellipsis"></omb-icon>
            </summary>
            <ul
              class="dropdown-content menu z-30 mt-1 w-52 rounded-box bg-base-100 shadow-lg border border-base-300 p-2"
            >
              <li>
                <button @click=${(e: Event) => this.#run(e, () => open("name"))}>
                  <omb-icon name="pencil"></omb-icon>Rename
                </button>
              </li>
              <li>
                <button @click=${(e: Event) => this.#run(e, () => open("variables"))}>
                  <omb-icon name="braces"></omb-icon>Variables
                </button>
              </li>
              <li class="mt-1 border-t border-base-300 pt-1">
                <button class="text-error" @click=${(e: Event) => this.#run(e, () => this.#delete())}>
                  <omb-icon name="trash"></omb-icon>Delete space…
                </button>
              </li>
            </ul>
          </details>
        </div>
        <omb-project-picker></omb-project-picker>
        <div class="flex items-center justify-end gap-2 flex-wrap">
          <span class="badge badge-outline gap-1.5 h-8 px-3"
            ><omb-icon name="git-fork" class="size-3.5"></omb-icon>${plural(totals.flows, "flow")}</span
          >
          ${
            totals.pending
              ? html`<span class="badge badge-warning badge-soft gap-1.5 h-8 px-3"
                  ><omb-icon name="clock" class="size-3.5"></omb-icon>${formatCount(totals.pending)}
                  pending</span
                >`
              : nothing
          }
          ${
            totals.errors
              ? html`<span class="badge badge-error badge-soft gap-1.5 h-8 px-3"
                  ><omb-icon name="circle-alert" class="size-3.5"></omb-icon
                  >${plural(totals.errors, "error")}</span
                >`
              : nothing
          }
        </div>
      </div>
    `;
  }
}
