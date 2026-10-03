import { describe, expect, it } from "vitest";
import { selectMedia, selectMediaRange, type MediaSelection } from "./media-selection";

const files = ["a", "b", "c", "d", "e"];
const empty: MediaSelection = { selected: new Set(), anchor: null };

describe("selectMedia", () => {
  it("selects a single item on a normal click", () => {
    expect(selectMedia(files, empty, "c")).toEqual({ selected: new Set(["c"]), anchor: "c" });
  });

  it("extends inclusively from the anchor in either direction", () => {
    const anchored = selectMedia(files, empty, "c");
    expect(selectMedia(files, anchored, "e", { extend: true }).selected).toEqual(new Set(["c", "d", "e"]));
    expect(selectMedia(files, anchored, "a", { extend: true }).selected).toEqual(new Set(["a", "b", "c"]));
  });

  it("toggles individual selections and unions modified range selections", () => {
    const selected = selectMedia(files, empty, "b", { toggle: true });
    expect(selectMedia(files, selected, "b", { toggle: true }).selected).toEqual(new Set());
    expect(selectMedia(files, selected, "d", { extend: true, toggle: true }).selected).toEqual(
      new Set(["b", "c", "d"]),
    );
  });

  it("replaces an invalid anchor when extending", () => {
    const stale = { selected: new Set(["missing"]), anchor: "missing" };
    expect(selectMedia(files, stale, "c", { extend: true })).toEqual({
      selected: new Set(["c"]),
      anchor: "c",
    });
  });

  it("ignores keys not in the current ordering", () => {
    expect(selectMedia(files, empty, "missing")).toBe(empty);
  });
});

describe("selectMediaRange", () => {
  it("returns an inclusive range regardless of endpoint order", () => {
    expect(selectMediaRange(files, "b", "d")).toEqual(new Set(["b", "c", "d"]));
    expect(selectMediaRange(files, "d", "b")).toEqual(new Set(["b", "c", "d"]));
  });

  it("returns no selection for missing endpoints", () => {
    expect(selectMediaRange(files, "missing", "b")).toEqual(new Set());
  });
});
