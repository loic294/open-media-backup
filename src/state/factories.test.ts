import { describe, expect, it } from "vitest";
import { newSpace } from "./factories";

describe("factories", () => {
  it("new spaces re-read every copy by default", () => {
    expect(newSpace("Travel", 0).verify_mode).toBe("reread");
  });
});
