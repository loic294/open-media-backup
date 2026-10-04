import { afterEach, describe, expect, it } from "vitest";
import { store } from "../../state";
import { OmbActiveJobsPanel } from "./active-jobs-panel";

describe("active jobs panel", () => {
  const previous = store.transfers;
  let element: OmbActiveJobsPanel;

  afterEach(() => {
    element?.remove();
    store.transfers = previous;
  });

  it("shows an empty state in standalone mode", async () => {
    store.transfers = [];
    element = new OmbActiveJobsPanel();
    element.standalone = true;
    document.body.append(element);
    await element.updateComplete;

    expect(element.textContent).toContain("No active jobs");
    expect(element.querySelector('[aria-label="Open Active jobs in a new window"]')).toBeNull();
  });
});
