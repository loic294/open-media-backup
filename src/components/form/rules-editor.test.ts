import { describe, expect, it } from "vitest";
import "./rules-editor";
import type { FileRule } from "../../api/types";
import type { OmbRulesEditor } from "./rules-editor";

describe("rules editor condition rules", () => {
  it("edits condition rules into backend expressions", async () => {
    const editor = document.createElement("omb-rules-editor") as OmbRulesEditor;
    const changes: FileRule[][] = [];
    editor.variables = ["client", "project_name"];
    editor.rules = [{ kind: "condition", expr: { op: "eq", var: "client", value: "Acme" } }];
    editor.addEventListener("rules-change", (event) =>
      changes.push((event as CustomEvent<FileRule[]>).detail),
    );
    document.body.append(editor);
    await editor.updateComplete;

    expect(editor.textContent).toContain('client = "Acme"');
    const addCondition = [...editor.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent?.trim() === "Add condition",
    )!;
    addCondition.click();
    await editor.updateComplete;

    const last = changes.at(-1)![0];
    expect(last).toEqual({
      kind: "condition",
      expr: {
        op: "and",
        items: [
          { op: "eq", var: "client", value: "Acme" },
          { op: "eq", var: "client", value: "" },
        ],
      },
    });
    expect(editor.textContent).toContain("AND");
    editor.remove();
  });
});
