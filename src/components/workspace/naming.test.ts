import { afterEach, describe, expect, it } from "vitest";
import { demoSnapshot } from "../../api/mock/data";
import { store } from "../../state";
import { OmbSourceCard } from "./source-card";
import { OmbDestinationCard } from "./destination-card";

describe("card task names", () => {
  const previousSnapshot = store.snapshot;
  const previousStatus = store.status;
  const cards: (OmbSourceCard | OmbDestinationCard)[] = [];

  afterEach(() => {
    cards.splice(0).forEach((card) => card.remove());
    store.snapshot = previousSnapshot;
    store.status = previousStatus;
  });

  it("differentiates sources on the same device and keeps hardware visible", async () => {
    const snapshot = demoSnapshot();
    store.snapshot = snapshot;
    store.status = null;
    const source = snapshot.sources[0];
    for (const [id, task_name] of [
      ["photos", "Photo ingest"],
      ["videos", "Video ingest"],
    ]) {
      const card = new OmbSourceCard();
      card.source = { ...source, id, task_name, backup_name: "Hidden backup name" };
      cards.push(card);
      document.body.append(card);
      await card.updateComplete;
      expect(card.querySelector(".font-semibold")!.textContent).toBe(task_name);
      expect(card.textContent).toContain("Device: Camera A · Card 1");
      expect(card.textContent).not.toContain("Hidden backup name");
    }
  });

  it.each(["d1", "d4"])(
    "shows the destination task title separately from device/app identity (%s)",
    async (id) => {
      const snapshot = demoSnapshot();
      store.snapshot = snapshot;
      store.status = null;
      const card = new OmbDestinationCard();
      card.destination = { ...snapshot.destinations.find((d) => d.id === id)!, task_name: "Photo archive" };
      cards.push(card);
      document.body.append(card);
      await card.updateComplete;
      expect(card.querySelector(".font-semibold")!.textContent).toContain("Photo archive");
      expect(card.textContent).toContain(
        id === "d1" ? "Device: Travel SSD" : "Lightroom · App · manual import",
      );
    },
  );
});
