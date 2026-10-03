import { html, nothing } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import type { CardContextMenuItemSpec } from "./card-context-menu-model";
import { OmbPureElement } from "../ui/omb-element";
import "../ui/omb-icon";

export interface CardContextMenuItem extends CardContextMenuItemSpec {
  run(): void;
}

const MENU_WIDTH = 240;
const MENU_MARGIN = 12;

@customElement("omb-card-context-menu")
export class OmbCardContextMenu extends OmbPureElement {
  @property({ attribute: false }) items: CardContextMenuItem[] = [];
  @property({ type: Number }) x = 0;
  @property({ type: Number }) y = 0;
  @state() private left = 0;
  @state() private top = 0;

  override connectedCallback(): void {
    super.connectedCallback();
    this.left = this.x;
    this.top = this.y;
    document.addEventListener("pointerdown", this.#onOutsidePointerDown, true);
    window.addEventListener("keydown", this.#onKeyDown);
    window.addEventListener("scroll", this.#close, true);
    window.addEventListener("blur", this.#close);
  }

  override disconnectedCallback(): void {
    document.removeEventListener("pointerdown", this.#onOutsidePointerDown, true);
    window.removeEventListener("keydown", this.#onKeyDown);
    window.removeEventListener("scroll", this.#close, true);
    window.removeEventListener("blur", this.#close);
    super.disconnectedCallback();
  }

  override firstUpdated(): void {
    this.#place();
    requestAnimationFrame(() => {
      this.#place();
      this.#enabledButtons()[0]?.focus();
    });
  }

  #place() {
    const menu = this.querySelector<HTMLElement>("[data-card-context-menu]");
    const width = menu?.offsetWidth || MENU_WIDTH;
    const height = menu?.offsetHeight || 190;
    this.left = Math.max(MENU_MARGIN, Math.min(this.x, window.innerWidth - width - MENU_MARGIN));
    this.top = Math.max(MENU_MARGIN, Math.min(this.y, window.innerHeight - height - MENU_MARGIN));
  }

  #enabledButtons(): HTMLButtonElement[] {
    return [...this.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
  }

  #close = () => {
    this.dispatchEvent(new CustomEvent("menu-close", { bubbles: true, composed: true }));
  };

  #onOutsidePointerDown = (event: PointerEvent) => {
    if (event.composedPath().includes(this)) return;
    this.#close();
  };

  #onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      this.#close();
      return;
    }
    const buttons = this.#enabledButtons();
    if (!buttons.length) return;
    const index = Math.max(0, buttons.indexOf(document.activeElement as HTMLButtonElement));
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : -1;
      buttons[(index + delta + buttons.length) % buttons.length].focus();
      return;
    }
    if (event.key === "Enter" && document.activeElement instanceof HTMLButtonElement) {
      event.preventDefault();
      document.activeElement.click();
    }
  };

  #select(item: CardContextMenuItem) {
    if (item.disabled) return;
    this.#close();
    item.run();
  }

  override render() {
    return html`
      <div
        data-card-context-menu
        role="menu"
        class="fixed z-50 w-60 rounded-box border border-base-300 bg-base-100 p-2 shadow-2xl"
        style="left:${this.left}px;top:${this.top}px"
        @pointerdown=${(event: Event) => event.stopPropagation()}
        @click=${(event: Event) => event.stopPropagation()}
        @contextmenu=${(event: Event) => event.preventDefault()}
      >
        <ul class="menu menu-sm p-0">
          ${this.items.map(
            (item) => html`
              ${
                item.separatorBefore
                  ? html`<li class="my-1 h-px bg-base-content/15" role="separator"></li>`
                  : nothing
              }
              <li>
                <button
                  type="button"
                  role="menuitem"
                  ?disabled=${item.disabled}
                  @click=${() => this.#select(item)}
                >
                  <omb-icon name=${item.icon} class="size-4 opacity-70"></omb-icon>${item.label}
                </button>
              </li>
            `,
          )}
        </ul>
      </div>
    `;
  }
}
