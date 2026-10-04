import { describe, expect, it } from "vitest";
import { expandTemplate, previewVars, templateParts, templateVars } from "./template";

describe("template", () => {
  it("extracts, splits and expands variables", () => {
    const t = "/volume1/photo/{backup_folder}/{project_name}";
    expect(templateVars(t)).toEqual(["backup_folder", "project_name"]);
    expect(templateParts(t).map((p) => p.variable)).toEqual([false, true, false, true]);
    expect(expandTemplate(t, { project_name: "Trip" })).toBe("/volume1/photo/{backup_folder}/Trip");
  });

  it("sanitizes effective backup names but leaves the unknown-source preview placeholder", () => {
    const space = { variables: [] };
    expect(previewVars(space, null, '.. Camera/A:*?"<>|\\B\u0001 ..').source_name).toBe(
      " Camera_A________B_ ",
    );
    expect(previewVars(space, null).source_name).toBe("{source_name}");
  });

  it("preserves the existing space/project override precedence", () => {
    const space = { variables: [{ name: "source_name", default_value: "Space override" }] };
    expect(previewVars(space, { name: "Project", values: {} }, "Backup").source_name).toBe("Space override");
    expect(
      previewVars(space, { name: "Project", values: { source_name: "Project override" } }, "Backup")
        .source_name,
    ).toBe("Project override");
  });
});
