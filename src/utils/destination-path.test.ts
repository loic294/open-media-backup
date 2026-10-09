import { describe, expect, it } from "vitest";
import type { DestinationPathPreview } from "../api/types";
import { destinationPathParts } from "./destination-path";

const destination = { path_template: "/2026/{client}/{project_name}", subfolder_per_source: false };
const previews: DestinationPathPreview[] = [
  {
    source_id: "s1",
    project_id: "a",
    path: "2026/Client A/Seattle",
    variables: { client: "Client A", project_name: "Seattle" },
  },
  {
    source_id: "s2",
    project_id: "b",
    path: "2026/Client B/Portland",
    variables: { client: "Client B", project_name: "Portland" },
  },
];

describe("destination path badges", () => {
  it("shows one coherent actual route with literals and per-variable alternatives", () => {
    const parts = destinationPathParts(destination, previews, null);
    expect(parts.map((part) => part.text).join("")).toBe(previews[0].path);
    expect(parts.filter((part) => part.name).map((part) => part.name)).toEqual(["client", "project_name"]);
    expect(parts.find((part) => part.name === "client")?.tooltip).toContain("Other variations: Client B");
    expect(parts.find((part) => part.name === "project_name")?.tooltip).toContain("{project_name}");
  });

  it("uses the selected incoming source without mixing it with another route", () => {
    const parts = destinationPathParts(destination, previews, "s2");
    expect(parts.map((part) => part.text).join("")).toBe(previews[1].path);
    expect(parts.find((part) => part.name === "client")?.tooltip).toContain("Client A");
    expect(
      destinationPathParts(destination, previews, "unknown")
        .map((part) => part.text)
        .join(""),
    ).toBe(previews[0].path);
  });

  it("does not fabricate values when a route is unavailable or invalid", () => {
    const parts = destinationPathParts(destination, [], null);
    expect(parts.map((part) => part.text).join("")).toBe("2026/{client}/{project_name}");
    for (const part of parts.filter((part) => part.name)) {
      expect(part.unresolved).toBe(true);
      expect(part.tooltip).toContain("Not resolved");
    }
  });

  it("prefers visible sources over a selected source hidden by the filter", () => {
    const parts = destinationPathParts(destination, previews, "s1", ["s2"]);
    expect(parts.map((part) => part.text).join("")).toBe(previews[1].path);
    expect(parts.find((part) => part.name === "client")?.tooltip).toContain("Client A");
    expect(previews[0].source_id).toBe("s1");
  });

  it("keeps a visible selection first, otherwise follows displayed source order", () => {
    expect(
      destinationPathParts(destination, previews, "s1", ["s2", "s1"])
        .map((part) => part.text).join(""),
    ).toBe(previews[0].path);
    expect(
      destinationPathParts(destination, previews, null, ["s2", "s1"])
        .map((part) => part.text).join(""),
    ).toBe(previews[1].path);
  });

  it("orders alternatives and source subfolders with visible routes before hidden routes", () => {
    const routes = [
      { ...previews[0], source_subfolder: "Hidden card" },
      { ...previews[1], source_subfolder: "Visible card" },
      {
        ...previews[0], source_id: "s3",
        variables: { client: "Client C", project_name: "Tacoma" },
        source_subfolder: "Selected card",
      },
    ];
    const parts = destinationPathParts(
      { ...destination, subfolder_per_source: true }, routes, "s3", ["s3", "s2"],
    );
    expect(parts.find((part) => part.name === "client")?.tooltip)
      .toContain("Other variations: Client B · Client A");
    expect(parts.at(-1)?.tooltip).toContain("Other variations: Visible card · Hidden card");
  });

  it("falls back to real routes when no visible source has a route", () => {
    for (const visible of [[], ["unknown"]]) {
      const parts = destinationPathParts(destination, previews, "s2", visible);
      expect(parts.map((part) => part.text).join("")).toBe(previews[0].path);
    }
  });

  it("deduplicates alternatives and preserves spaces, case and repeated variables", () => {
    const parts = destinationPathParts(
      { path_template: "{ client }/{client}", subfolder_per_source: false },
      [...previews, previews[1]],
      null,
    );
    expect(parts.map((part) => part.text).join("")).toBe("Client A/Client A");
    expect(parts[0].tooltip).toBe("{client}\nValue: Client A\nOther variations: Client B");
  });

  it("uses separate source subfolder values, not a project's source_name override", () => {
    const parts = destinationPathParts(
      { path_template: "{source_name}", subfolder_per_source: true },
      [
        {
          ...previews[0],
          variables: { source_name: "Custom project value" },
          source_subfolder: "Sony Card 1",
        },
      ],
      null,
    );
    expect(parts.map((part) => part.text).join("")).toBe("Custom project value/Sony Card 1");
    expect(parts.at(-1)?.tooltip).toContain("subfolder per source");
  });

  it("normalizes only the portable relative display path, including separators and traversal", () => {
    const parts = destinationPathParts(
      { path_template: "\\2026//./../{backup_folder}\\", subfolder_per_source: false },
      [{ ...previews[0], variables: { backup_folder: "Photo Archive\\Seattle/./../RAW" } }],
      null,
    );
    expect(parts.map((part) => part.text).join("")).toBe("2026/Photo Archive/Seattle/RAW");
    expect(parts.filter((part) => part.name)).toHaveLength(1);
    expect(parts.find((part) => part.name === "backup_folder")?.text).toBe("Photo Archive/Seattle/RAW");
  });
});
