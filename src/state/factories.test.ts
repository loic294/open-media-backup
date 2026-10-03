import { describe, expect, it } from "vitest";
import { newSpace } from "./factories";

describe("factories", () => {
  it("new spaces re-read every copy by default", () => {
    const space = newSpace("Travel", 0);
    expect(space.hash_algo).toBe("blake3");
    expect(space.verify_mode).toBe("reread");
  });
});
