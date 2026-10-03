import { describe, expect, it } from "vitest";
import { assetNameFromUrl, compareSemver, selectPlatform } from "../src/index";

describe("compareSemver", () => {
  it("sorts normal versions and ignores a leading v", () => {
    expect(compareSemver("v0.2.0", "0.1.9")).toBe(1);
    expect(compareSemver("1.2.0", "1.2")).toBe(0);
    expect(compareSemver("1.2.3", "1.3.0")).toBe(-1);
  });

  it("treats stable releases as newer than prereleases", () => {
    expect(compareSemver("1.0.0", "1.0.0-beta.1")).toBe(1);
    expect(compareSemver("1.0.0-alpha.2", "1.0.0-alpha.10")).toBe(-1);
  });
});

describe("selectPlatform", () => {
  const manifest = {
    version: "0.2.0",
    platforms: {
      "darwin-aarch64": {
        url: "https://github.com/loic294/open-media-backup/releases/download/v0.2.0/app-aarch64.app.tar.gz",
        signature: "sig-a",
      },
      "windows-x86_64": {
        url: "https://github.com/loic294/open-media-backup/releases/download/v0.2.0/app-x64.msi",
        signature: "sig-w",
      },
    },
  };

  it("selects by target-arch key", () => {
    expect(selectPlatform(manifest, "darwin", "aarch64")?.signature).toBe("sig-a");
    expect(selectPlatform(manifest, "linux", "x86_64")).toBeNull();
  });

  it("extracts the installer asset name", () => {
    expect(
      assetNameFromUrl("https://example.com/releases/download/v0.2.0/Open%20Media%20Backup.dmg?download=1"),
    ).toBe("Open Media Backup.dmg");
  });
});
