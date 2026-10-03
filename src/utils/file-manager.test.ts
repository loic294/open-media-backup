import { describe, expect, it, vi } from "vitest";
import { fileManagerName } from "./file-manager";

function withNavigator(platform: string, userAgent: string, run: () => void) {
  vi.spyOn(navigator, "platform", "get").mockReturnValue(platform);
  vi.spyOn(navigator, "userAgent", "get").mockReturnValue(userAgent);
  try {
    run();
  } finally {
    vi.restoreAllMocks();
  }
}

describe("fileManagerName", () => {
  it("returns Finder on macOS", () => {
    withNavigator("MacIntel", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", () => {
      expect(fileManagerName()).toBe("Finder");
    });
  });

  it("returns File Explorer on Windows", () => {
    withNavigator("Win32", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)", () => {
      expect(fileManagerName()).toBe("File Explorer");
    });
  });

  it("returns a generic name elsewhere", () => {
    withNavigator("Linux x86_64", "Mozilla/5.0 (X11; Linux x86_64)", () => {
      expect(fileManagerName()).toBe("file manager");
    });
  });
});
