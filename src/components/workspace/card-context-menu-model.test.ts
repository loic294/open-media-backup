import { describe, expect, it } from "vitest";
import { buildCardContextMenuItems, revealLabel, runFlowsLabel } from "./card-context-menu-model";

describe("card context menu model", () => {
  it("pluralizes run flow labels", () => {
    expect(runFlowsLabel(0)).toBe("Run 0 flows");
    expect(runFlowsLabel(1)).toBe("Run 1 flow");
    expect(runFlowsLabel(2)).toBe("Run 2 flows");
  });

  it("uses platform-specific reveal labels", () => {
    expect(revealLabel("Finder")).toBe("View in Finder");
    expect(revealLabel("File Explorer")).toBe("View in File Explorer");
    expect(revealLabel("file manager")).toBe("Open in file manager");
  });

  it("disables run when there are no runnable flows or the drive is offline", () => {
    expect(
      buildCardContextMenuItems({
        fileManagerName: "Finder",
        runnableCount: 0,
        offline: false,
        hasFilesystemPath: true,
      }).find((item) => item.action === "run")?.disabled,
    ).toBe(true);
    expect(
      buildCardContextMenuItems({
        fileManagerName: "Finder",
        runnableCount: 2,
        offline: true,
        hasFilesystemPath: true,
      }).find((item) => item.action === "run")?.disabled,
    ).toBe(true);
  });

  it("hides reveal for app destinations or cards without a filesystem path", () => {
    expect(
      buildCardContextMenuItems({
        fileManagerName: "Finder",
        runnableCount: 1,
        offline: false,
        hasFilesystemPath: true,
        isAppDestination: true,
      }).some((item) => item.action === "reveal"),
    ).toBe(false);
    expect(
      buildCardContextMenuItems({
        fileManagerName: "Finder",
        runnableCount: 1,
        offline: false,
        hasFilesystemPath: false,
      }).some((item) => item.action === "reveal"),
    ).toBe(false);
  });

  it("shows but disables reveal for offline filesystem cards", () => {
    const reveal = buildCardContextMenuItems({
      fileManagerName: "File Explorer",
      runnableCount: 1,
      offline: true,
      hasFilesystemPath: true,
    }).find((item) => item.action === "reveal");
    expect(reveal).toMatchObject({
      label: "View in File Explorer",
      disabled: true,
      separatorBefore: true,
    });
  });

  it("offers Check only for folder destinations and respects its availability", () => {
    const state = {
      fileManagerName: "Finder" as const,
      runnableCount: 0,
      offline: false,
      hasFilesystemPath: true,
      checkDisabled: false,
    };
    expect(buildCardContextMenuItems(state).find((item) => item.action === "check")?.disabled).toBe(false);
    expect(
      buildCardContextMenuItems({ ...state, checkDisabled: true }).find((item) => item.action === "check")
        ?.disabled,
    ).toBe(true);
    expect(
      buildCardContextMenuItems({ ...state, isAppDestination: true }).some((item) => item.action === "check"),
    ).toBe(false);
  });
});
