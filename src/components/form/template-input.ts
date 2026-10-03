import { html, nothing } from "lit";
import { customElement, property } from "lit/decorators.js";
import { expandTemplate, templateParts } from "../../utils/template";
import { OmbPureElement } from "../ui/omb-element";

/** Path input supporting {variables}: highlights known/unknown variables and previews the result. */
@customElement("omb-template-input")
export class OmbTemplateInput extends OmbPureElement {
  @property() label = "";
  @property() value = "";
  @property() placeholder = "";
  @property() prefix = "";
  @property() hint = "";
  @property({ attribute: false }) vars: Record<string, string> = {};

  #input(e: Event) {
    this.value = (e.target as HTMLInputElement).value;
    this.dispatchEvent(new CustomEvent("value-change", { detail: this.value }));
  }

  #insert(name: string) {
    this.value = `${this.value}{${name}}`;
    this.dispatchEvent(new CustomEvent("value-change", { detail: this.value }));
  }

  override render() {
    const parts = templateParts(this.value);
    const unknown = parts.filter((p) => p.variable && !(p.text.slice(1, -1) in this.vars));
    const resolved = expandTemplate(this.value, this.vars);
    return html`
      <fieldset class="fieldset">
        ${this.label ? html`<legend class="fieldset-legend">${this.label}</legend>` : nothing}
        <label class="input w-full font-mono text-sm">
          ${this.prefix ? html`<span class="text-base-content/50 truncate max-w-48">${this.prefix}/</span>` : nothing}
          <input class="grow" .value=${this.value} placeholder=${this.placeholder} @input=${this.#input} spellcheck="false" />
        </label>
        <div class="flex flex-wrap items-center gap-1 mt-1">
          ${Object.keys(this.vars)
            .filter((k) => !this.value.includes(`{${k}}`))
            .slice(0, 8)
            .map((k) => html`<button type="button" class="badge badge-sm badge-ghost font-mono hover:badge-primary" @click=${() => this.#insert(k)}>{${k}}</button>`)}
        </div>
        ${this.value
          ? html`<p class="text-xs mt-1 font-mono break-all">
              <span class="text-base-content/50">→ ${this.prefix ? `${this.prefix}/` : ""}</span>${templateParts(resolved).map((p) =>
                p.variable ? html`<span class="text-error">${p.text}</span>` : html`<span class="text-base-content/80">${p.text}</span>`,
              )}
            </p>`
          : nothing}
        ${unknown.length ? html`<p class="text-xs text-error">Unknown: ${unknown.map((u) => u.text).join(", ")}. Define it in the space variables.</p>` : nothing}
        ${this.hint ? html`<p class="label">${this.hint}</p>` : nothing}
      </fieldset>
    `;
  }
}
