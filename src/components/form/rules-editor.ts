import { html, nothing } from "lit";
import { customElement, property } from "lit/decorators.js";
import type { FileRule, PathFileRule, RuleAction, RuleSyntax } from "../../api/types";
import {
  conditionDraftFromExpr,
  conditionRuleFromDraft,
  conditionRuleSummary,
  defaultConditionDraft,
  type ConditionCombinator,
  type ConditionOperator,
  type FileRuleConditionClause,
  type FileRuleConditionDraft,
} from "../../utils/file-rule-conditions";
import { OmbPureElement } from "../ui/omb-element";

export function ruleError(rule: FileRule): string | null {
  if (isConditionRule(rule) || rule.syntax !== "regex" || !rule.pattern) return null;
  try {
    new RegExp(rule.pattern);
    return null;
  } catch (e) {
    return (e as Error).message;
  }
}

function isConditionRule(rule: FileRule): rule is Extract<FileRule, { kind: "condition" }> {
  return "kind" in rule && rule.kind === "condition";
}

/** Ordered include/exclude rules (glob or regex) applied to folder and file names. Last match wins. */
@customElement("omb-rules-editor")
export class OmbRulesEditor extends OmbPureElement {
  @property({ attribute: false }) rules: FileRule[] = [];
  @property({ attribute: false }) variables: string[] = [];

  #emit(rules: FileRule[]) {
    this.rules = rules;
    this.dispatchEvent(new CustomEvent("rules-change", { detail: rules }));
  }

  #patch(i: number, rule: FileRule) {
    this.#emit(this.rules.map((r, j) => (j === i ? rule : r)));
  }

  #patchPath(i: number, patch: Partial<PathFileRule>) {
    const rule = this.rules[i];
    if (!rule || isConditionRule(rule)) return;
    this.#patch(i, { ...rule, ...patch });
  }

  #patchCondition(i: number, draft: FileRuleConditionDraft) {
    this.#patch(i, conditionRuleFromDraft(draft));
  }

  #defaultVariable() {
    return this.variables[0] ?? "";
  }

  #conditionOptions(draft: FileRuleConditionDraft) {
    return [
      ...new Set([...this.variables, ...draft.clauses.map((clause) => clause.varName).filter(Boolean)]),
    ];
  }

  #typeSelect(rule: FileRule, i: number) {
    return html`<select
      class="select select-sm w-28"
      .value=${isConditionRule(rule) ? "condition" : "path"}
      aria-label="Rule type"
      @change=${(e: Event) => {
        const type = (e.target as HTMLSelectElement).value;
        this.#patch(
          i,
          type === "condition"
            ? conditionRuleFromDraft(defaultConditionDraft(this.#defaultVariable()))
            : { action: "exclude", syntax: "glob", pattern: "" },
        );
      }}
    >
      <option value="path">Path</option>
      <option value="condition">Condition</option>
    </select>`;
  }

  #pathRow(rule: PathFileRule, i: number) {
    const error = ruleError(rule);
    return html`
      <div class="flex flex-col gap-2 sm:flex-row sm:items-center">
        ${this.#typeSelect(rule, i)}
        <select
          class="select select-sm w-full sm:w-28"
          .value=${rule.action}
          @change=${(e: Event) => this.#patchPath(i, { action: (e.target as HTMLSelectElement).value as RuleAction })}
        >
          <option value="include">Include</option>
          <option value="exclude">Exclude</option>
        </select>
        <select
          class="select select-sm w-full sm:w-24"
          .value=${rule.syntax}
          @change=${(e: Event) => this.#patchPath(i, { syntax: (e.target as HTMLSelectElement).value as RuleSyntax })}
        >
          <option value="glob">Glob</option>
          <option value="regex">Regex</option>
        </select>
        <input
          class="input input-sm flex-1 font-mono ${error ? "input-error" : ""}"
          .value=${rule.pattern}
          placeholder=${rule.syntax === "glob" ? "*.ARW or PRIVATE/" : "^IMG_\\d+\\.(ARW|JPG)$"}
          title=${error ?? ""}
          @input=${(e: Event) => this.#patchPath(i, { pattern: (e.target as HTMLInputElement).value })}
        />
        <button
          type="button"
          class="btn btn-ghost btn-sm btn-square self-end sm:self-auto"
          title="Remove rule"
          @click=${() => this.#emit(this.rules.filter((_, j) => j !== i))}
        >
          <omb-icon name="x"></omb-icon>
        </button>
      </div>
    `;
  }

  #conditionClause(
    ruleIndex: number,
    draft: FileRuleConditionDraft,
    clause: FileRuleConditionClause,
    i: number,
  ) {
    const patchClause = (patch: Partial<FileRuleConditionClause>) => {
      const clauses = draft.clauses.map((item, j) => (j === i ? { ...item, ...patch } : item));
      this.#patchCondition(ruleIndex, { ...draft, clauses });
    };
    const options = this.#conditionOptions(draft);
    return html`
      <div
        class="grid gap-2 sm:grid-cols-[auto_minmax(0,1fr)_minmax(9rem,11rem)_minmax(0,1fr)_auto] sm:items-center"
      >
        <label
          class="btn btn-sm ${clause.negated ? "btn-primary" : "btn-ghost"} justify-start sm:justify-center"
        >
          <input
            type="checkbox"
            class="sr-only"
            .checked=${clause.negated}
            @change=${(e: Event) => patchClause({ negated: (e.target as HTMLInputElement).checked })}
          />
          NOT
        </label>
        <select
          class="select select-sm w-full"
          .value=${clause.varName}
          aria-label="Project variable"
          @change=${(e: Event) => patchClause({ varName: (e.target as HTMLSelectElement).value })}
        >
          ${
            options.length
              ? options.map((name) => html`<option value=${name}>${name}</option>`)
              : html`<option value="">No variables</option>`
          }
        </select>
        <select
          class="select select-sm w-full"
          .value=${clause.operator}
          aria-label="Condition operator"
          @change=${(e: Event) => patchClause({ operator: (e.target as HTMLSelectElement).value as ConditionOperator })}
        >
          <option value="eq">equals</option>
          <option value="ne">does not equal</option>
        </select>
        <input
          class="input input-sm w-full"
          .value=${clause.value}
          placeholder="Value"
          aria-label="Condition value"
          @input=${(e: Event) => patchClause({ value: (e.target as HTMLInputElement).value })}
        />
        <button
          type="button"
          class="btn btn-ghost btn-sm btn-square justify-self-end"
          title="Remove condition"
          ?disabled=${draft.clauses.length === 1}
          @click=${() => this.#patchCondition(ruleIndex, { ...draft, clauses: draft.clauses.filter((_, j) => j !== i) })}
        >
          <omb-icon name="x"></omb-icon>
        </button>
      </div>
    `;
  }

  #combinatorToggle(ruleIndex: number, draft: FileRuleConditionDraft) {
    const setCombinator = (combinator: ConditionCombinator) =>
      this.#patchCondition(ruleIndex, { ...draft, combinator });
    return html`<div class="join" role="group" aria-label="Condition combinator">
      <button
        type="button"
        class="btn btn-sm join-item min-w-16 px-4 ${draft.combinator === "and" ? "btn-primary" : "btn-ghost"}"
        @click=${() => setCombinator("and")}
      >
        AND
      </button>
      <button
        type="button"
        class="btn btn-sm join-item min-w-16 px-4 ${draft.combinator === "or" ? "btn-primary" : "btn-ghost"}"
        @click=${() => setCombinator("or")}
      >
        OR
      </button>
    </div>`;
  }

  #conditionRow(rule: Extract<FileRule, { kind: "condition" }>, i: number) {
    const draft = conditionDraftFromExpr(rule.expr);
    return html`
      <div class="rounded-box border border-base-300 bg-base-100 p-3">
        <div class="flex flex-col gap-3 sm:flex-row sm:items-center">
          ${this.#typeSelect(rule, i)}
          <span
            class="badge badge-outline badge-lg h-auto min-h-8 max-w-full whitespace-normal rounded-full px-3 py-1 text-left leading-snug"
          >
            ${conditionRuleSummary(rule)}
          </span>
          <button
            type="button"
            class="btn btn-ghost btn-sm btn-square ml-auto"
            title="Remove rule"
            @click=${() => this.#emit(this.rules.filter((_, j) => j !== i))}
          >
            <omb-icon name="x"></omb-icon>
          </button>
        </div>
        <div class="mt-3 rounded-box border border-base-300 p-3">
          <div class="mb-3 flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <h5 class="font-medium">Rule conditions</h5>
            ${draft.clauses.length > 1 ? this.#combinatorToggle(i, draft) : nothing}
          </div>
          <div class="flex flex-col gap-2">
            ${draft.clauses.map((clause, j) => this.#conditionClause(i, draft, clause, j))}
          </div>
          <button
            type="button"
            class="btn btn-sm btn-ghost mt-3 gap-1"
            @click=${() =>
              this.#patchCondition(i, {
                ...draft,
                clauses: [
                  ...draft.clauses,
                  { varName: this.#defaultVariable(), operator: "eq", value: "", negated: false },
                ],
              })}
          >
            <omb-icon name="plus"></omb-icon>Add condition
          </button>
        </div>
      </div>
    `;
  }

  #row(rule: FileRule, i: number) {
    return isConditionRule(rule) ? this.#conditionRow(rule, i) : this.#pathRow(rule, i);
  }

  override render() {
    return html`
      <div class="flex flex-col gap-3">
        ${this.rules.map((r, i) => this.#row(r, i))}
        <div class="flex flex-col gap-2 sm:flex-row sm:items-center">
          <button
            type="button"
            class="btn btn-sm btn-ghost gap-1"
            @click=${() => this.#emit([...this.rules, { action: "exclude", syntax: "glob", pattern: "" }])}
          >
            <omb-icon name="plus"></omb-icon>Add path rule
          </button>
          <button
            type="button"
            class="btn btn-sm btn-ghost gap-1"
            @click=${() => this.#emit([...this.rules, conditionRuleFromDraft(defaultConditionDraft(this.#defaultVariable()))])}
          >
            <omb-icon name="plus"></omb-icon>Add condition
          </button>
          <span class="text-xs text-base-content/50">
            Path rules match file and folder names; condition rules filter by project variables.
          </span>
        </div>
      </div>
    `;
  }
}
