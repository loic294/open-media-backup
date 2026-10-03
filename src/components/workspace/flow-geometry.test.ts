import { describe, expect, it } from "vitest";
import { curve, entryPoints, pointOnCurve } from "./flow-geometry";

describe("flow geometry", () => {
  const a = { x: 0, y: 0 };
  const b = { x: 200, y: 100 };

  it("draws a horizontal S-curve", () => {
    expect(curve(a, b)).toBe("M 0 0 C 100 0, 100 100, 200 100");
  });

  it("evaluates curve end points and midpoint", () => {
    expect(pointOnCurve(a, b, 0)).toEqual(a);
    expect(pointOnCurve(a, b, 1)).toEqual(b);
    expect(pointOnCurve(a, b, 0.5)).toEqual({ x: 100, y: 50 });
  });

  it("spreads entry points around the box middle", () => {
    const box = { top: 0, left: 10, right: 100, height: 100 };
    expect(entryPoints(box, 0)).toEqual([]);
    expect(entryPoints(box, 1)).toEqual([{ x: 10, y: 50 }]);
    expect(entryPoints(box, 3).map((p) => p.y)).toEqual([24, 50, 76]);
  });
});
