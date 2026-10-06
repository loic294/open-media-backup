import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LitElement } from "lit";
import appConfigJson from "../../../src-tauri/tauri.conf.json?raw";
import demoConfigJson from "../../../src-tauri/tauri.demo.conf.json?raw";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { OmbTopBar } from "./omb-top-bar";
import "../ui/omb-icon";

const chrome = vi.hoisted(() => ({
  platform: null as "macos" | "windows" | "linux" | null,
  preview: false,
}));

vi.mock("../../utils/platform", () => ({
  nativeChromePlatform: () => chrome.platform,
  isChromePreview: () => chrome.preview,
}));

describe("window header", () => {
  const previousSnapshot = store.snapshot;
  const previousDialogs = store.dialogs;
  let bar: OmbTopBar;

  beforeEach(() => {
    store.snapshot = demoSnapshot();
    store.dialogs = [];
    chrome.platform = null;
    chrome.preview = false;
    bar = new OmbTopBar();
  });

  afterEach(() => {
    bar.remove();
    store.snapshot = previousSnapshot;
    store.dialogs = previousDialogs;
    vi.restoreAllMocks();
  });

  async function render() {
    document.body.append(bar);
    await bar.updateComplete;
    await Promise.all(
      [...bar.querySelectorAll<LitElement>("omb-space-pills, omb-sync-pill, omb-icon")].map(
        (element) => element.updateComplete,
      ),
    );
    return bar.querySelector("header")!;
  }

  it.each([null, "macos", "windows", "linux"] as const)(
    "uses one deep drag region without self-only descendant overrides on %s",
    async (platform) => {
      chrome.platform = platform;
      chrome.preview = platform !== null;
      const header = await render();
      expect(header.getAttribute("data-tauri-drag-region")).toBe("deep");
      expect(header.classList.contains("h-16")).toBe(true);
      expect(header.querySelector("[data-tauri-drag-region]")).toBeNull();
      expect(header.querySelector('omb-icon[name="shield-check"] svg')).not.toBeNull();
      expect(header.classList.contains("pl-24")).toBe(platform === "macos");
      expect(header.querySelector('[aria-label="Window controls"]') !== null).toBe(platform === "windows");
    },
  );

  it("keeps nested header controls interactive without explicit drag markers", async () => {
    await render();
    const open = vi.spyOn(store, "open");
    const selectSpace = vi.spyOn(store, "selectSpace").mockResolvedValue();
    const settingsIcon = bar.querySelector('button[title="Settings"] omb-icon')!;
    settingsIcon.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(open).toHaveBeenCalledWith({ type: "app-settings" });
    bar.querySelector<HTMLElement>("omb-sync-pill button span")!.click();
    expect(open).toHaveBeenCalledWith({ type: "device-sync" });
    bar.querySelector<HTMLButtonElement>("omb-space-pills button")!.click();
    expect(selectSpace).toHaveBeenCalledWith(store.snapshot!.spaces[0].id);
    for (const button of bar.querySelectorAll("button")) {
      expect(button.hasAttribute("data-tauri-drag-region")).toBe(false);
    }
  });

  it("reserves macOS native controls without adding a desktop overlay", async () => {
    chrome.platform = "macos";
    const header = await render();
    expect(header.querySelector('div.absolute[aria-hidden="true"]')).toBeNull();
    chrome.preview = true;
    bar.requestUpdate();
    await bar.updateComplete;
    expect(header.querySelector('div.absolute[aria-hidden="true"]')?.classList.contains("top-1/2")).toBe(
      true,
    );
  });

  it.each([
    { name: "normal", json: appConfigJson },
    { name: "demo", json: demoConfigJson },
  ])("centers native controls in the 64px header in the $name configuration", ({ json }) => {
    const config = JSON.parse(json);
    const main = config.app.windows.find((window: { label: string }) => window.label === "main");
    expect(main.titleBarStyle).toBe("Overlay");
    expect(main.hiddenTitle).toBe(true);
    expect(main.trafficLightPosition).toEqual({ x: 20, y: 30 });
    // AppKit's 16px button frame retains its 6px bottom offset in Tauri's container.
    expect(main.trafficLightPosition.y + 16 / 2 - 6).toBe(64 / 2);
  });
});
