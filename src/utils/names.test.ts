import { describe, expect, it } from "vitest";
import { demoSnapshot } from "../api/mock/data";
import { flowLabel } from "../state/selectors";
import { destinationTaskName, sourceBackupName, sourceTaskName } from "./names";

describe("independent naming", () => {
  it.each([undefined, "", " \t "])("falls back for legacy or blank names (%s)", (blank) => {
    const snapshot = demoSnapshot();
    const source = { ...snapshot.sources[0], task_name: blank, backup_name: blank };
    const device = snapshot.devices.find((d) => d.id === source.device_id)!;
    expect(sourceTaskName(source, device)).toBe(device.name);
    expect(sourceBackupName(source, device)).toBe(device.name);
    expect(destinationTaskName({ ...snapshot.destinations[0], task_name: blank }, device)).toBe(device.name);
    expect(destinationTaskName({ ...snapshot.destinations[3], task_name: blank })).toBe("Lightroom");
  });

  it("resolves separate task and backup overrides without changing the shared device", () => {
    const snapshot = demoSnapshot();
    const device = snapshot.devices[0];
    const first = { ...snapshot.sources[0], task_name: " Photos ", backup_name: " Camera A " };
    const second = { ...first, id: "second", task_name: "Videos", backup_name: "Camera B" };
    expect(sourceTaskName(first, device)).toBe("Photos");
    expect(sourceTaskName(second, device)).toBe("Videos");
    expect(sourceBackupName(first, device)).toBe("Camera A");
    expect(sourceBackupName(second, device)).toBe("Camera B");
    expect(device.name).toBe("Camera A · Card 1");
  });

  it("keeps blank fallbacks dynamic when the physical device is renamed", () => {
    const snapshot = demoSnapshot();
    const source = snapshot.sources[0];
    const device = { ...snapshot.devices[0], name: "Renamed hardware" };
    expect(sourceTaskName(source, device)).toBe("Renamed hardware");
    expect(sourceBackupName(source, device)).toBe("Renamed hardware");
    expect(sourceTaskName({ ...source, task_name: "Fixed task" }, device)).toBe("Fixed task");
  });

  it("uses task labels for folder and app flows, never backup names", () => {
    const snapshot = demoSnapshot();
    snapshot.sources[0].task_name = "Photo ingest";
    snapshot.sources[0].backup_name = "Path only";
    snapshot.destinations[0].task_name = "Travel RAW";
    snapshot.destinations[3].task_name = "Photo import";
    expect(flowLabel(snapshot, snapshot.flows[0])).toBe("Photo ingest → Travel RAW");
    expect(
      flowLabel(
        snapshot,
        snapshot.flows.find((f) => f.id === "f7")!,
      ),
    ).toBe("Photo ingest → Photo import");
  });

  it("falls back to the local app display name when its shared name is absent", () => {
    const destination = { ...demoSnapshot().destinations[3], app_name: " ", task_name: "" };
    expect(destinationTaskName(destination, undefined, "/Applications/Capture One.app")).toBe("Capture One");
    expect(destinationTaskName(destination)).toBe("Application");
  });
});
