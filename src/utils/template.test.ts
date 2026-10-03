import { describe, expect, it } from "vitest";
import { expandTemplate, templateParts, templateVars } from "./template";

describe("template", () => {
  it("extracts, splits and expands variables", () => {
    const t = "/volume1/photo/{backup_folder}/{project_name}";
    expect(templateVars(t)).toEqual(["backup_folder", "project_name"]);
    expect(templateParts(t).map((p) => p.variable)).toEqual([false, true, false, true]);
    expect(expandTemplate(t, { project_name: "Trip" })).toBe("/volume1/photo/{backup_folder}/Trip");
  });
});
