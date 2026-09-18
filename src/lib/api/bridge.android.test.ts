import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), readFile: vi.fn(), writeFile: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: mocks.readFile, writeFile: mocks.writeFile }));

beforeEach(() => {
  vi.resetAllMocks();
  vi.stubGlobal("navigator", { userAgent: "Android" });
});
afterEach(() => vi.unstubAllGlobals());

describe("Android attachment export", () => {
  it("passes only the path to native scoped-storage export without reading file bytes", async () => {
    mocks.invoke.mockResolvedValue({ savedPath: "content://media/external/downloads/42" });
    const { downloadFile } = await import("./bridge");
    expect(await downloadFile("/data/user/0/app/files/received/report.pdf")).toBe(
      "content://media/external/downloads/42",
    );
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("plugin:file-helper|saveToDownloads", {
      path: "/data/user/0/app/files/received/report.pdf",
    });
    expect(mocks.readFile).not.toHaveBeenCalled();
    expect(mocks.writeFile).not.toHaveBeenCalled();
  });

  it("surfaces native cancellation and storage errors", async () => {
    mocks.invoke.mockRejectedValue(new Error("Save cancelled"));
    const { downloadFile } = await import("./bridge");
    await expect(downloadFile("/attachment.pdf")).rejects.toThrow("Save cancelled");
    expect(mocks.readFile).not.toHaveBeenCalled();
  });

  it("does not report success when Android returns no destination", async () => {
    mocks.invoke.mockResolvedValue({});
    const { downloadFile } = await import("./bridge");
    await expect(downloadFile("/attachment.pdf")).rejects.toThrow("saved destination");
  });
});
