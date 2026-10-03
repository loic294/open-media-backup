import { describe, expect, it } from "vitest";
import { appPickerOptions } from "./app-picker";

describe("appPickerOptions", () => {
  it("lets macOS users pick the .app bundle itself", () => {
    expect(appPickerOptions("macOS")).toEqual({
      defaultPath: "/Applications",
      canCreateDirectories: false,
      filters: [{ name: "Applications", extensions: ["app"] }],
    });
  });

  it("filters Windows apps to launchable app file types", () => {
    expect(appPickerOptions("Windows")).toEqual({
      filters: [{ name: "Applications", extensions: ["exe", "lnk"] }],
    });
  });

  it("keeps Linux app picking unrestricted so executables can be selected", () => {
    expect(appPickerOptions("Linux")).toEqual({});
  });
});
