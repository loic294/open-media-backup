import { html } from "lit";
import { customElement } from "lit/decorators.js";
import { newSpace } from "../../state/factories";
import { nextPosition, sortedSpaces } from "../../state/selectors";
import { OmbElement } from "../ui/omb-element";

/** Space switcher. "+" creates a space and opens its settings. */
@customElement("omb-space-pills")
export class OmbSpacePills extends OmbElement {
  async #add() {
    const snapshot = this.store.snapshot;
    if (!snapshot) return;
    const space = newSpace("New space", nextPosition(snapshot.spaces));
    await this.store.save("space", space);
    await this.store.selectSpace(space.id);
    this.store.open({ type: "space-settings", spaceId: space.id, focus: "name" });
  }

  override render() {
    const snapshot = this.store.snapshot;
    if (!snapshot) return null;
    const active = this.store.space?.id;
    return html`
      <nav class="flex items-center gap-1 p-1 rounded-full bg-base-100 border border-base-300" aria-label="Spaces">
        ${sortedSpaces(snapshot).map(
          (s) => html`
            <button
              class="btn btn-sm rounded-full border-0 gap-2 ${s.id === active ? "btn-primary" : "btn-ghost"}"
              aria-pressed=${s.id === active}
              @click=${() => this.store.selectSpace(s.id)}
            >
              <omb-icon name=${s.icon}></omb-icon>${s.name}
            </button>
          `,
        )}
        <button class="btn btn-sm btn-ghost btn-circle" title="Add space" @click=${() => this.#add()}>
          <omb-icon name="plus"></omb-icon>
        </button>
      </nav>
    `;
  }
}
