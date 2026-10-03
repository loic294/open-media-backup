import type { IconName } from "../ui/icons";

export type CardContextMenuAction = "browse" | "run" | "edit" | "reveal";

export interface CardContextMenuItemSpec {
  action: CardContextMenuAction;
  label: string;
  icon: IconName;
  disabled?: boolean;
  separatorBefore?: boolean;
}

export interface CardContextMenuState {
  fileManagerName: "Finder" | "File Explorer" | "file manager";
  runnableCount: number;
  offline: boolean;
  hasFilesystemPath: boolean;
  isAppDestination?: boolean;
  browseDisabled?: boolean;
}

export function revealLabel(fileManager: CardContextMenuState["fileManagerName"]): string {
  return fileManager === "file manager" ? "Open in file manager" : `View in ${fileManager}`;
}

export function runFlowsLabel(count: number): string {
  return `Run ${count.toLocaleString("en-US")} ${count === 1 ? "flow" : "flows"}`;
}

export function buildCardContextMenuItems(state: CardContextMenuState): CardContextMenuItemSpec[] {
  const items: CardContextMenuItemSpec[] = [
    {
      action: "browse",
      label: "Browse media",
      icon: "images",
      disabled: state.browseDisabled,
    },
    {
      action: "run",
      label: runFlowsLabel(state.runnableCount),
      icon: "play",
      disabled: state.offline || state.runnableCount === 0,
    },
    {
      action: "edit",
      label: "Edit…",
      icon: "pencil",
    },
  ];
  if (!state.isAppDestination && state.hasFilesystemPath) {
    items.push({
      action: "reveal",
      label: revealLabel(state.fileManagerName),
      icon: "external-link",
      disabled: state.offline,
      separatorBefore: true,
    });
  }
  return items;
}
