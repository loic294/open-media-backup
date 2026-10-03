import type { ConditionFileRule, RuleExpr } from "../api/types";

export type ConditionOperator = "eq" | "ne";
export type ConditionCombinator = "and" | "or";

export interface FileRuleConditionClause {
  varName: string;
  operator: ConditionOperator;
  value: string;
  negated: boolean;
}

export interface FileRuleConditionDraft {
  combinator: ConditionCombinator;
  clauses: FileRuleConditionClause[];
}

export function defaultConditionDraft(varName = ""): FileRuleConditionDraft {
  return {
    combinator: "and",
    clauses: [{ varName, operator: "eq", value: "", negated: false }],
  };
}

function exprToClause(expr: RuleExpr): FileRuleConditionClause | null {
  if (expr.op === "not") {
    const clause = exprToClause(expr.item);
    return clause ? { ...clause, negated: !clause.negated } : null;
  }
  if (expr.op === "eq" || expr.op === "ne") {
    return { varName: expr.var, operator: expr.op, value: expr.value, negated: false };
  }
  return null;
}

export function conditionDraftFromExpr(expr: RuleExpr): FileRuleConditionDraft {
  if (expr.op === "and" || expr.op === "or") {
    const clauses = expr.items
      .map(exprToClause)
      .filter((clause): clause is FileRuleConditionClause => !!clause);
    return clauses.length ? { combinator: expr.op, clauses } : defaultConditionDraft();
  }
  const clause = exprToClause(expr);
  return { combinator: "and", clauses: [clause ?? defaultConditionDraft().clauses[0]] };
}

export function conditionExprFromDraft(draft: FileRuleConditionDraft): RuleExpr {
  const clauses = draft.clauses.length ? draft.clauses : defaultConditionDraft().clauses;
  const items = clauses.map((clause): RuleExpr => {
    const base: RuleExpr = { op: clause.operator, var: clause.varName, value: clause.value };
    return clause.negated ? { op: "not", item: base } : base;
  });
  return items.length === 1 ? items[0] : { op: draft.combinator, items };
}

export function conditionRuleFromDraft(draft: FileRuleConditionDraft): ConditionFileRule {
  return { kind: "condition", expr: conditionExprFromDraft(draft) };
}

function quoteValue(value: string): string {
  return `"${value.replace(/"/g, '\\"')}"`;
}

export function conditionSummaryFromDraft(draft: FileRuleConditionDraft): string {
  const joiner = ` ${draft.combinator.toUpperCase()} `;
  return draft.clauses
    .map((clause) => {
      const variable = clause.varName || "variable";
      const operator = clause.operator === "eq" ? "=" : "≠";
      const comparison = `${variable} ${operator} ${quoteValue(clause.value)}`;
      return clause.negated ? `NOT ${comparison}` : comparison;
    })
    .join(joiner);
}

export function conditionRuleSummary(rule: ConditionFileRule): string {
  return conditionSummaryFromDraft(conditionDraftFromExpr(rule.expr));
}
