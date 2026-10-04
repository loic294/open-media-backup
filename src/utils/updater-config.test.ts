import { describe, expect, it } from "vitest";
import appConfigJson from "../../src-tauri/tauri.conf.json?raw";
import demoConfigJson from "../../src-tauri/tauri.demo.conf.json?raw";
import releaseConfigJson from "../../src-tauri/tauri.release.conf.json?raw";

describe("updater configuration", () => {
  it("uses the managed domain over HTTPS and preserves the dynamic update route", () => {
    const config = JSON.parse(appConfigJson);

    expect(config.plugins.updater.endpoints).toEqual([
      "https://update-open-media-backup.loicba.me/{{target}}/{{arch}}/{{current_version}}",
    ]);
    expect(config.plugins.updater.dangerousInsecureTransportProtocol).not.toBe(true);
  });

  it("only signs updater artifacts in release builds", () => {
    expect(JSON.parse(appConfigJson).bundle.createUpdaterArtifacts).toBe(false);
    expect(JSON.parse(releaseConfigJson).bundle.createUpdaterArtifacts).toBe(true);
  });

  it("does not override the updater configuration in demo builds", () => {
    const config = JSON.parse(demoConfigJson);

    expect(config.plugins?.updater).toBeUndefined();
  });
});
