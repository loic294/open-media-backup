import { createElement } from "lucide";
import { html, nothing } from "lit";
import { customElement, property } from "lit/decorators.js";
import { ICONS, type IconName } from "./icons";
import { OmbPureElement } from "./omb-element";

/** <omb-icon name="plane" class="size-4"> — a Lucide icon that inherits currentColor. */
@customElement("omb-icon")
export class OmbIcon extends OmbPureElement {
  @property() name: IconName | string = "file";
  @property({ type: Number }) stroke = 2;

  override render() {
    const node = ICONS[this.name as IconName] ?? ICONS.file;
    if (!node) return nothing;
    const svg = createElement(node, { "stroke-width": this.stroke, width: "100%", height: "100%", "aria-hidden": "true" });
    return html`${svg}`;
  }

  override connectedCallback(): void {
    super.connectedCallback();
    this.classList.add("inline-block", "shrink-0");
    if (![...this.classList].some((c) => c.startsWith("size-"))) this.classList.add("size-4");
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "omb-icon": OmbIcon;
  }
}
