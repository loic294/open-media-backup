import type { Destination, DestinationPathPreview } from "../api/types";
import { templateParts } from "./template";

export interface DestinationPathPart {
  text: string;
  name?: string;
  tooltip?: string;
  unresolved?: boolean;
}

/** Keep values from one real route together; never combine unrelated project/source values. */
export function destinationPathParts(
  destination: Pick<Destination, "path_template" | "subfolder_per_source">,
  previews: readonly DestinationPathPreview[],
  selectedSourceId: string | null,
): DestinationPathPart[] {
  const primary = previews.find((preview) => preview.source_id === selectedSourceId) ?? previews[0];
  const parts = templateParts(destination.path_template).map((part): DestinationPathPart => {
    if (!part.variable) return { text: part.text };
    const name = part.text.slice(1, -1).trim();
    const value = primary?.variables[name];
    const variations = [...new Set(previews.map((preview) => preview.variables[name]))].filter(
      (other): other is string => other !== undefined && other !== value,
    );
    const tooltip = [
      `{${name}}`,
      value === undefined
        ? "Not resolved: no transferable or completed route is available."
        : `Value: ${value}`,
      variations.length ? `Other variations: ${variations.join(" · ")}` : "No other variations.",
    ].join("\n");
    return {
      text: value ?? part.text,
      name,
      tooltip,
      unresolved: value === undefined,
    };
  });
  if (destination.subfolder_per_source) {
    const value = primary?.source_subfolder ?? undefined;
    const variations = [...new Set(previews.map((preview) => preview.source_subfolder))].filter(
      (other): other is string => other != null && other !== value,
    );
    parts.push(
      { text: "/" },
      {
        text: value ?? "{source_name}",
        name: "source_name",
        unresolved: value === undefined,
        tooltip: [
          "{source_name} (subfolder per source)",
          value === undefined ? "Not resolved: no route is available." : `Value: ${value}`,
          variations.length ? `Other variations: ${variations.join(" · ")}` : "No other variations.",
        ].join("\n"),
      },
    );
  }

  // Transfer base paths are portable: normalize separators and discard empty/traversal segments.
  const segments: { parts: DestinationPathPart[]; separator: DestinationPathPart }[] = [
    { parts: [], separator: { text: "/" } },
  ];
  for (const part of parts) {
    for (const [index, text] of part.text.split(/[/\\]/).entries()) {
      if (index) segments.push({ parts: [], separator: { ...part, text: "/" } });
      if (text) segments.at(-1)!.parts.push({ ...part, text });
    }
  }
  const normalized = segments
    .filter((segment) => {
      const text = segment.parts.map((part) => part.text).join("");
      return text !== "" && text !== "." && text !== "..";
    })
    .flatMap((segment, index) => (index ? [segment.separator, ...segment.parts] : segment.parts));
  const result: DestinationPathPart[] = [];
  for (const part of normalized) {
    const previous = result.at(-1);
    if (previous?.name && previous.name === part.name && previous.tooltip === part.tooltip) {
      previous.text += part.text;
    } else {
      result.push({ ...part });
    }
  }
  return result;
}
