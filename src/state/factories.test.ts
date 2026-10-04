import { describe, expect, it } from "vitest";
import { newDestination, newSource, newSpace } from "./factories";

describe("factories", () => {
  it("new spaces re-read every copy by default", () => {
    const space = newSpace("Travel", 0);
    expect(space.hash_algo).toBe("blake3");
    expect(space.verify_mode).toBe("reread");
  });

  it("new tasks keep names blank so they follow the device until overridden", () => {
    expect(newSource("space", "device", 0)).toMatchObject({ task_name: "", backup_name: "" });
    expect(newDestination("space", "device", 0)).toMatchObject({ task_name: "" });
  });
});
