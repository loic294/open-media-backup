import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

const WINDOW_LABEL = "active-jobs";
let openingWindow: Promise<void> | null = null;

export function openActiveJobsWindow(): Promise<void> {
  if (openingWindow) return openingWindow;
  openingWindow = showActiveJobsWindow().finally(() => {
    openingWindow = null;
  });
  return openingWindow;
}

async function showActiveJobsWindow(): Promise<void> {
  const existingWindow = await WebviewWindow.getByLabel(WINDOW_LABEL);
  if (existingWindow) {
    await focusWindow(existingWindow);
    return;
  }

  const window = new WebviewWindow(WINDOW_LABEL, {
    url: "/?ombWindow=active-jobs",
    title: "Active jobs",
    width: 560,
    height: 640,
    minWidth: 420,
    minHeight: 320,
    resizable: true,
    center: true,
  });
  await new Promise<void>((resolve, reject) => {
    void window.once("tauri://created", () => resolve());
    void window.once("tauri://error", (event) => reject(new Error(String(event.payload))));
  });
  await focusWindow(window);
}

async function focusWindow(window: WebviewWindow): Promise<void> {
  await window.unminimize();
  await window.show();
  await window.setFocus();
}
