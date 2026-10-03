import { html } from "lit";
import { customElement, property } from "lit/decorators.js";
import type { FileRule } from "../../api/types";
import { OmbPureElement } from "../ui/omb-element";

export function ruleError(rule: FileRule): string | null {
  if (rule.syntax !== "regex" || !rule.pattern) return null;
  try {
    new RegExp(rule.pattern);
    return null;
  } catch (e) {
    return (e as Error).message;
  }
}

/** Ordered include/exclude rules (glob or regex) applied to folder and file names. Last match wins. */
@customElement("omb-rules-editor")
export class OmbRulesEditor extends OmbPureElement {
  @property({ attribute: false }) rules: FileRule[] = [];

  #emit(rules: FileRule[]) {
    this.rules = rules;
    this.dispatchEvent(new CustomEvent("rules-change", { detail: rules }));
  }

  #patch(i: number, patch: Partial<FileRule>) {
    this.#emit(this.rules.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  }

  #row(rule: FileRule, i: number) {
    const error = ruleError(rule);
    return html`
      <div class="flex items-center gap-2">
        <select class="select select-sm w-28" .value=${rule.action} @change=${(e: Event) => this.#patch(i, { action: (e.target as HTMLSelectElement).value as FileRule["action"] })}>
          <option value="include">Include</option>
          <option value="exclude">Exclude</option>
        </select>
        <select class="select select-sm w-24" .value=${rule.syntax} @change=${(e: Event) => this.#patch(i, { syntax: (e.target as HTMLSelectElement).value as FileRule["syntax"] })}>
          <option value="glob">Glob</option>
          <option value="regex">Regex</option>
        </select>
        <input
          class="input input-sm flex-1 font-mono ${error ? "input-error" : ""}"
          .value=${rule.pattern}
          placeholder=${rule.syntax === "glob" ? "*.ARW or PRIVATE/" : "^DSC\\d+\\.(ARW|JPG)$"}
          title=${error ?? ""}
          @input=${(e: Event) => this.#patch(i, { pattern: (e.target as HTMLInputElement).value })}
        />
        <button type="button" class="btn btn-ghost btn-sm btn-square" title="Remove rule" @click=${() => this.#emit(this.rules.filter((_, j) => j !== i))}>
          <omb-icon name="x"></omb-icon>
        </button>
      </div>
    `;
  }

  override render() {
    return html`
      <div class="flex flex-col gap-2">
        ${this.rules.map((r, i) => this.#row(r, i))}
        <div class="flex items-center gap-2">
          <button type="button" class="btn btn-sm btn-ghost gap-1" @click=${() => this.#emit([...this.rules, { action: "exclude", syntax: "glob", pattern: "" }])}>
            <omb-icon name="plus"></omb-icon>Add rule
          </button>
          <span class="text-xs text-base-content/50">
            Rules match file and folder names; a trailing <code>/</code> matches folders only. The last matching rule wins. With no include rule, everything is included.
          </span>
        </div>
      </div>
    `;
  }
}
