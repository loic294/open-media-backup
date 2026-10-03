export interface MediaSelection {
  selected: Set<string>;
  anchor: string | null;
}

export interface MediaSelectionGesture {
  extend?: boolean;
  toggle?: boolean;
}

export function selectMedia(
  orderedKeys: readonly string[],
  current: MediaSelection,
  key: string,
  gesture: MediaSelectionGesture = {},
): MediaSelection {
  const index = orderedKeys.indexOf(key);
  if (index < 0) return current;

  if (gesture.extend && current.anchor) {
    const anchorIndex = orderedKeys.indexOf(current.anchor);
    if (anchorIndex < 0) {
      return { selected: new Set([key]), anchor: key };
    }

    const start = Math.min(index, anchorIndex);
    const end = Math.max(index, anchorIndex);
    const selected = gesture.toggle ? new Set(current.selected) : new Set<string>();
    for (const rangeKey of orderedKeys.slice(start, end + 1)) selected.add(rangeKey);
    return { selected, anchor: current.anchor };
  }

  if (gesture.toggle) {
    const selected = new Set(current.selected);
    if (selected.has(key)) selected.delete(key);
    else selected.add(key);
    return { selected, anchor: key };
  }

  return { selected: new Set([key]), anchor: key };
}

export function selectMediaRange(
  orderedKeys: readonly string[],
  startKey: string,
  endKey: string,
): Set<string> {
  const start = orderedKeys.indexOf(startKey);
  const end = orderedKeys.indexOf(endKey);
  if (start < 0 || end < 0) return new Set();
  return new Set(orderedKeys.slice(Math.min(start, end), Math.max(start, end) + 1));
}
