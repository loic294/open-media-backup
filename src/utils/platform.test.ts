import { describe, expect, it } from "vitest";
import { detectDesktopPlatform, previewChromePlatform } from "./platform";

describe("desktop platform helpers", () => {
  it("detects native desktop platforms from navigator values", () => {
    expect(detectDesktopPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", "MacIntel")).toBe(
      "macos",
    );
    expect(detectDesktopPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", "Win32")).toBe("windows");
    expect(detectDesktopPlatform("Mozilla/5.0 (X11; Linux x86_64)", "Linux x86_64")).toBe("linux");
    expect(detectDesktopPlatform("Mozilla/5.0", "Unknown")).toBeNull();
  });

  it("only enables chrome previews when demo mode is enabled", () => {
    expect(previewChromePlatform("?ombChrome=macos", true)).toBe("macos");
    expect(previewChromePlatform("?ombChrome=windows", true)).toBe("windows");
    expect(previewChromePlatform("?ombChrome=beos", true)).toBeNull();
    expect(previewChromePlatform("?ombChrome=macos", false)).toBeNull();
  });
});
