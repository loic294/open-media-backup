import { afterEach, describe, expect, it } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { OmbDeviceField } from "./device-field";

describe("device field ordering", () => {
  const previousSnapshot = store.snapshot;
  const previousVolumes = store.volumes;
  let field: OmbDeviceField | undefined;

  afterEach(() => {
    field?.remove();
    field = undefined;
    store.snapshot = previousSnapshot;
    store.volumes = previousVolumes;
  });

  it("puts mounted devices first by default and preserves existing order when disabled", async () => {
    const snapshot = demoSnapshot();
    store.snapshot = snapshot;
    store.volumes = [
      {
        mount_path: "/Volumes/photo",
        name: "photo",
        volume_uuid: null,
        hw_serial: null,
        total_bytes: null,
        free_bytes: null,
        removable: false,
        device_id: "nas",
        matched_by: "mapping",
      },
    ];
    field = new OmbDeviceField();
    field.deviceId = "ssd";
    document.body.append(field);
    await field.updateComplete;

    const optionIds = () =>
      [...field!.querySelector("select")!.querySelectorAll<HTMLOptionElement>("option")]
        .filter((option) => option.value !== "__new")
        .map((option) => option.value);
    expect(optionIds()).toEqual(["nas", "card1", "card2", "dji", "ssd", "hdd"]);

    snapshot.settings.show_mounted_devices_first = false;
    await store.saveSettings({ show_mounted_devices_first: false });
    await field.updateComplete;

    expect(optionIds()).toEqual(["card1", "card2", "dji", "ssd", "nas", "hdd"]);
  });
});
