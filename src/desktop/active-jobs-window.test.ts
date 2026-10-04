import { beforeEach, describe, expect, it, vi } from "vitest";
import { openActiveJobsWindow } from "./active-jobs-window";

const windowMocks = vi.hoisted(() => ({
  instances: [] as Array<{
    label: string;
    options: Record<string, unknown>;
    show: () => Promise<void>;
    unminimize: () => Promise<void>;
    setFocus: () => Promise<void>;
  }>,
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: class {
    label: string;

    constructor(label: string, options: Record<string, unknown>) {
      this.label = label;
      const instance = {
        label,
        options,
        show: vi.fn(async () => {}),
        unminimize: vi.fn(async () => {}),
        setFocus: vi.fn(async () => {}),
      };
      windowMocks.instances.push(instance);
      queueMicrotask(() => this.#created?.({ payload: null }));
      this.#instance = instance;
    }

    #instance: (typeof windowMocks.instances)[number];
    #created?: (event: { payload: unknown }) => void;

    static async getByLabel(label: string) {
      return windowMocks.instances.find((instance) => instance.label === label) ?? null;
    }

    once(event: string, handler: (event: { payload: unknown }) => void) {
      if (event === "tauri://created") this.#created = handler;
      return Promise.resolve(() => {});
    }

    show() {
      return this.#instance.show();
    }

    unminimize() {
      return this.#instance.unminimize();
    }

    setFocus() {
      return this.#instance.setFocus();
    }
  },
}));

describe("openActiveJobsWindow", () => {
  beforeEach(() => {
    windowMocks.instances = [];
  });

  it("creates the dedicated window once and focuses it on subsequent opens", async () => {
    await openActiveJobsWindow();
    await openActiveJobsWindow();

    expect(windowMocks.instances).toHaveLength(1);
    expect(windowMocks.instances[0].options).toMatchObject({
      url: "/?ombWindow=active-jobs",
      title: "Active jobs",
    });
    expect(windowMocks.instances[0].unminimize).toHaveBeenCalledTimes(2);
    expect(windowMocks.instances[0].show).toHaveBeenCalledTimes(2);
    expect(windowMocks.instances[0].setFocus).toHaveBeenCalledTimes(2);
  });
});
