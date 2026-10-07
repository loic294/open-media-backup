import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const { jpegBytes } = await import("./tauri-backend");

describe("jpegBytes", () => {
  it("accepts every raw IPC payload shape", () => {
    expect([...jpegBytes(new Uint8Array([0xff, 0xd8]).buffer)]).toEqual([0xff, 0xd8]);
    expect([...jpegBytes([0xff, 0xd8, 0xff])]).toEqual([0xff, 0xd8, 0xff]);
    expect([...jpegBytes(new Uint8Array([1, 2]))]).toEqual([1, 2]);
    expect(jpegBytes([]).byteLength).toBe(0);
    expect(jpegBytes(null).byteLength).toBe(0);
  });
});
