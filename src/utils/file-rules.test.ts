import { describe, expect, it } from "vitest";
import {
  appendExcludeRule,
  exactFilenameExcludePattern,
  extensionExcludePattern,
  extensionForRule,
  rulesAllowPath,
} from "./file-rules";
import type { FileRule } from "../api/types";

describe("file rule helpers", () => {
  it("escapes glob metacharacters in exact filename rules", () => {
    const pattern = exactFilenameExcludePattern("IMG_[01]{raw}?.JPG");
    expect(pattern).toBe("IMG_\\[01\\]\\{raw\\}\\?.JPG");
    expect(rulesAllowPath([{ action: "exclude", syntax: "glob", pattern }], "DCIM/IMG_[01]{raw}?.JPG")).toBe(
      false,
    );
    expect(rulesAllowPath([{ action: "exclude", syntax: "glob", pattern }], "DCIM/IMG_Araw1.JPG")).toBe(true);
  });

  it("builds case-insensitive extension rules", () => {
    const pattern = extensionExcludePattern("IMG_0001.JPG");
    expect(pattern).toBe("*.JPG");
    expect(
      rulesAllowPath([{ action: "exclude", syntax: "glob", pattern: pattern! }], "100MSDCF/other.jpg"),
    ).toBe(false);
  });

  it("does not offer extension rules for no-extension files", () => {
    expect(extensionForRule("README")).toBeNull();
    expect(extensionExcludePattern("README")).toBeNull();
    expect(extensionExcludePattern(".DS_Store")).toBeNull();
    expect(extensionExcludePattern("trailing.")).toBeNull();
  });

  it("deduplicates equivalent glob exclude rules", () => {
    const rules: FileRule[] = [{ action: "exclude", syntax: "glob", pattern: "*.jpg" }];
    expect(appendExcludeRule(rules, "*.JPG")).toEqual({ rules, added: false });
    expect(appendExcludeRule(rules, "IMG_0001.JPG")).toEqual({
      rules: [...rules, { action: "exclude", syntax: "glob", pattern: "IMG_0001.JPG" }],
      added: true,
    });
  });

  it("applies condition rules as normalized project-variable filters", () => {
    const rules: FileRule[] = [
      { action: "include", syntax: "glob", pattern: "*.jpg" },
      {
        kind: "condition",
        expr: {
          op: "and",
          items: [
            { op: "eq", var: "client", value: "acme" },
            { op: "not", item: { op: "eq", var: "status", value: "draft" } },
            { op: "or", items: [{ op: "ne", var: "missing", value: "present" }] },
          ],
        },
      },
    ];

    expect(rulesAllowPath(rules, "DCIM/A.JPG", { client: " Acme ", status: "Ready" })).toBe(true);
    expect(rulesAllowPath(rules, "DCIM/A.ARW", { client: "Acme", status: "Ready" })).toBe(false);
    expect(rulesAllowPath(rules, "DCIM/A.JPG", { client: "Other", status: "Ready" })).toBe(false);
    expect(
      rulesAllowPath([{ kind: "condition", expr: { op: "eq", var: "missing", value: "" } }], "A.JPG"),
    ).toBe(true);
  });
});
