import { describe, expect, it } from "vitest";
import {
  conditionDraftFromExpr,
  conditionExprFromDraft,
  conditionRuleFromDraft,
  conditionRuleSummary,
  conditionSummaryFromDraft,
  type FileRuleConditionDraft,
} from "./file-rule-conditions";

const draft: FileRuleConditionDraft = {
  combinator: "and",
  clauses: [
    { varName: "Camera", operator: "eq", value: "A7IV", negated: false },
    { varName: "Client", operator: "eq", value: "Acme", negated: true },
  ],
};

describe("file rule condition helpers", () => {
  it("builds backend expressions with eq/ne/not/and/or", () => {
    expect(conditionExprFromDraft(draft)).toEqual({
      op: "and",
      items: [
        { op: "eq", var: "Camera", value: "A7IV" },
        { op: "not", item: { op: "eq", var: "Client", value: "Acme" } },
      ],
    });

    expect(
      conditionExprFromDraft({
        combinator: "or",
        clauses: [
          { varName: "client", operator: "ne", value: "Internal", negated: false },
          { varName: "status", operator: "eq", value: "Ready", negated: false },
        ],
      }),
    ).toEqual({
      op: "or",
      items: [
        { op: "ne", var: "client", value: "Internal" },
        { op: "eq", var: "status", value: "Ready" },
      ],
    });
  });

  it("formats readable summaries", () => {
    expect(conditionSummaryFromDraft(draft)).toBe('Camera = "A7IV" AND NOT Client = "Acme"');
    expect(
      conditionSummaryFromDraft({
        combinator: "or",
        clauses: [{ varName: "client", operator: "ne", value: 'A "Team"', negated: false }],
      }),
    ).toBe('client ≠ "A \\"Team\\""');
  });

  it("round-trips editable expression shapes", () => {
    const expr = {
      op: "or" as const,
      items: [
        { op: "ne" as const, var: "client", value: "Internal" },
        { op: "not" as const, item: { op: "eq" as const, var: "status", value: "Draft" } },
      ],
    };
    expect(conditionExprFromDraft(conditionDraftFromExpr(expr))).toEqual(expr);
    expect(conditionRuleFromDraft(draft)).toEqual({ kind: "condition", expr: conditionExprFromDraft(draft) });
    expect(conditionRuleSummary(conditionRuleFromDraft(draft))).toBe(
      'Camera = "A7IV" AND NOT Client = "Acme"',
    );
  });
});
