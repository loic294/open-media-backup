import { afterEach, expect, it, vi } from "vitest";
import type { FileEntry } from "../../api/types";
import { store } from "../../state";
import { OmbThumbnail } from "./omb-thumbnail";

afterEach(() => {
  document.body.replaceChildren();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("loads only visible thumbnails and ignores an old image after a tile changes file", async () => {
  let intersect!: IntersectionObserverCallback;
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      constructor(callback: IntersectionObserverCallback) {
        intersect = callback;
      }
      observe() {}
      disconnect() {}
    },
  );
  let resolveOld!: (value: string | null) => void;
  const thumbnail = vi
    .spyOn(store.backend, "thumbnail")
    .mockImplementationOnce(() => new Promise((resolve) => (resolveOld = resolve)))
    .mockResolvedValueOnce("blob:new");
  const file: FileEntry = {
    rel_path: "old.jpg",
    name: "old.jpg",
    size: 100,
    media: "image",
    abs_path: "/old.jpg",
    target_path: null,
    category: "to_transfer",
    error: null,
  };
  const tile = new OmbThumbnail();
  tile.file = file;
  document.body.append(tile);
  await tile.updateComplete;
  expect(thumbnail).not.toHaveBeenCalled();
  intersect([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver);
  expect(thumbnail).toHaveBeenCalledWith("/old.jpg");
  tile.file = { ...file, name: "new.jpg", rel_path: "new.jpg", abs_path: "/new.jpg" };
  await tile.updateComplete;
  await new Promise((resolve) => setTimeout(resolve, 0));
  resolveOld("blob:old");
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(tile.querySelector("img")?.getAttribute("src")).toBe("blob:new");
  expect(tile.querySelector("img")?.getAttribute("alt")).toBe("new.jpg");
});
