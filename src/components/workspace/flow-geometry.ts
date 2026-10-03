export interface Point {
  x: number;
  y: number;
}

export interface Box {
  top: number;
  left: number;
  right: number;
  height: number;
}

/** Horizontal S-curve between two ports. */
export function curve(a: Point, b: Point): string {
  const dx = Math.max(40, Math.abs(b.x - a.x) * 0.5);
  return `M ${a.x} ${a.y} C ${a.x + dx} ${a.y}, ${b.x - dx} ${b.y}, ${b.x} ${b.y}`;
}

/** Point on the curve() bezier at parameter t (0..1), used to place badges. */
export function pointOnCurve(a: Point, b: Point, t: number): Point {
  const dx = Math.max(40, Math.abs(b.x - a.x) * 0.5);
  const p1 = { x: a.x + dx, y: a.y };
  const p2 = { x: b.x - dx, y: b.y };
  const u = 1 - t;
  const w = [u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t];
  return {
    x: w[0] * a.x + w[1] * p1.x + w[2] * p2.x + w[3] * b.x,
    y: w[0] * a.y + w[1] * p1.y + w[2] * p2.y + w[3] * b.y,
  };
}

/** Spreads n incoming connections vertically around the middle of a destination card's left edge. */
export function entryPoints(box: Box, n: number): Point[] {
  if (n <= 0) return [];
  const spacing = Math.min(26, (box.height - 40) / Math.max(1, n - 1));
  const mid = box.top + box.height / 2;
  const start = mid - (spacing * (n - 1)) / 2;
  return Array.from({ length: n }, (_, i) => ({ x: box.left, y: start + i * spacing }));
}
