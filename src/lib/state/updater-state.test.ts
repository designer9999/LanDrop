import { describe, expect, it, vi } from "vitest";
import type { DownloadEvent } from "@tauri-apps/plugin-updater";
import { UpdaterState } from "./updater-state.svelte";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function updateResource(version = "2.0.0") {
  return {
    version,
    body: "Release notes",
    download: vi.fn(async (_callback?: (event: DownloadEvent) => void) => {}),
    install: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
  };
}

function setup() {
  const update = updateResource();
  const check = vi.fn(async () => update as ReturnType<typeof updateResource> | null);
  const relaunch = vi.fn(async () => {});
  const prepareToExit = vi.fn(async () => {});
  let block = "";
  const state = new UpdaterState({ check, relaunch, prepareToExit, blockedReason: () => block });
  return {
    state,
    update,
    check,
    relaunch,
    prepareToExit,
    setBlock: (reason: string) => {
      block = reason;
    },
  };
}

describe("desktop update consent and lifecycle", () => {
  it("allows explicit installation while notification delivery is still pending", async () => {
    const { state, update } = setup();
    const notification = deferred<void>();
    await state.check(() => notification.promise);
    expect(state.phase).toBe("available");
    expect(await state.install()).toBe(true);
    expect(update.install).toHaveBeenCalledOnce();
    notification.resolve();
  });

  it("awaits persistence before exiting and rechecks transfers after the flush", async () => {
    const { state, update, prepareToExit, setBlock } = setup();
    const persisted = deferred<void>();
    prepareToExit.mockReturnValueOnce(persisted.promise);
    await state.check();
    const installing = state.install();
    await vi.waitFor(() => expect(prepareToExit).toHaveBeenCalled());
    expect(update.install).not.toHaveBeenCalled();
    setBlock("New incoming transfer");
    persisted.resolve();
    expect(await installing).toBe(false);
    expect(state.phase).toBe("downloaded");
    expect(update.install).not.toHaveBeenCalled();
    setBlock("");
    expect(await state.install()).toBe(true);
    expect(update.download).toHaveBeenCalledOnce();
  });

  it("does not install or relaunch if saving state fails", async () => {
    const { state, update, prepareToExit, relaunch } = setup();
    prepareToExit.mockRejectedValueOnce(new Error("disk full"));
    await state.check();
    expect(await state.install()).toBe(false);
    expect(state.error).toContain("disk full");
    expect(update.install).not.toHaveBeenCalled();
    expect(update.close).toHaveBeenCalledOnce();
    expect(relaunch).not.toHaveBeenCalled();
  });

  it("flushes state before explicit restart and protects a draft created during the flush", async () => {
    const { state, prepareToExit, relaunch, setBlock } = setup();
    await state.check();
    await state.install();
    const persisted = deferred<void>();
    prepareToExit.mockReturnValueOnce(persisted.promise);
    const restarting = state.restart();
    expect(relaunch).not.toHaveBeenCalled();
    setBlock("New draft");
    persisted.resolve();
    expect(await restarting).toBe(false);
    expect(relaunch).not.toHaveBeenCalled();
    setBlock("");
    expect(await state.restart()).toBe(true);
  });

  it("checks without downloading, installing, or restarting and isolates notification errors", async () => {
    const { state, update, relaunch } = setup();
    const notify = vi.fn(async () => {
      throw new Error("notifications unavailable");
    });
    await state.check(notify);
    expect(state.phase).toBe("available");
    expect(notify).toHaveBeenCalledWith("2.0.0");
    expect(update.download).not.toHaveBeenCalled();
    expect(update.install).not.toHaveBeenCalled();
    expect(relaunch).not.toHaveBeenCalled();
    expect(state.error).toBe("");
  });

  it("deduplicates concurrent checks and closes replaced update resources", async () => {
    const { state, update, check } = setup();
    const pending = deferred<ReturnType<typeof updateResource> | null>();
    check.mockReturnValueOnce(pending.promise);
    const first = state.check();
    expect(state.check()).toBe(first);
    pending.resolve(update);
    await first;
    check.mockResolvedValueOnce(null);
    await state.check();
    expect(check).toHaveBeenCalledTimes(2);
    expect(update.close).toHaveBeenCalledTimes(1);
    expect(state.phase).toBe("idle");
    expect(state.hasChecked).toBe(true);
  });

  it("tracks download bytes and waits for explicit restart after installing", async () => {
    const { state, update, relaunch } = setup();
    update.download.mockImplementationOnce(async (callback) => {
      callback?.({ event: "Started", data: { contentLength: 100 } });
      callback?.({ event: "Progress", data: { chunkLength: 40 } });
      expect(state.progressPercent).toBe(40);
      callback?.({ event: "Progress", data: { chunkLength: 60 } });
      callback?.({ event: "Finished" });
    });
    await state.check();
    expect(await state.install()).toBe(true);
    expect(update.install).toHaveBeenCalledWith({ restartAfterInstall: true });
    expect(update.close).toHaveBeenCalledTimes(1);
    expect(state.phase).toBe("ready");
    expect(relaunch).not.toHaveBeenCalled();
    expect(await state.restart()).toBe(true);
    expect(relaunch).toHaveBeenCalledTimes(1);
  });

  it("blocks installs while a transfer or unsent draft exists", async () => {
    const { state, update, setBlock } = setup();
    await state.check();
    setBlock("Wait for the transfer to finish");
    expect(await state.install()).toBe(false);
    setBlock("Send or clear your draft");
    expect(await state.install()).toBe(false);
    expect(state.error).toBe("Send or clear your draft");
    expect(update.download).not.toHaveBeenCalled();
  });

  it("deduplicates installs and rechecks safety after download without redownloading on resume", async () => {
    const { state, update, check, setBlock } = setup();
    const pending = deferred<void>();
    update.download.mockReturnValueOnce(pending.promise);
    await state.check();
    const first = state.install();
    expect(state.install()).toBe(first);
    await state.check();
    expect(check).toHaveBeenCalledTimes(1);
    setBlock("Incoming transfer in progress");
    pending.resolve();
    expect(await first).toBe(false);
    expect(state.phase).toBe("downloaded");
    expect(update.install).not.toHaveBeenCalled();
    setBlock("");
    expect(await state.install()).toBe(true);
    expect(update.download).toHaveBeenCalledTimes(1);
  });

  it("closes a failed update and obtains a fresh resource for an explicit retry", async () => {
    const { state, update, check } = setup();
    update.download.mockRejectedValueOnce(new Error("signature invalid"));
    await state.check();
    expect(await state.install()).toBe(false);
    expect(state.error).toContain("signature invalid");
    expect(state.errorPhase).toBe("install");
    expect(update.install).not.toHaveBeenCalled();
    expect(update.close).toHaveBeenCalledTimes(1);
    const retry = updateResource();
    check.mockResolvedValueOnce(retry);
    expect(await state.install()).toBe(true);
    expect(retry.download).toHaveBeenCalledTimes(1);
    expect(retry.install).toHaveBeenCalledTimes(1);
  });

  it("releases a check result that arrives after disposal without notifying", async () => {
    const { state, update, check } = setup();
    const pending = deferred<ReturnType<typeof updateResource> | null>();
    check.mockReturnValueOnce(pending.promise);
    const notify = vi.fn();
    const checking = state.check(notify);
    await vi.waitFor(() => expect(check).toHaveBeenCalled());
    await state.dispose();
    pending.resolve(update);
    await checking;
    expect(update.close).toHaveBeenCalledTimes(1);
    expect(notify).not.toHaveBeenCalled();
  });

  it("does not install a download that finishes after disposal", async () => {
    const { state, update } = setup();
    const pending = deferred<void>();
    update.download.mockReturnValueOnce(pending.promise);
    await state.check();
    const installing = state.install();
    await state.dispose();
    pending.resolve();
    expect(await installing).toBe(false);
    expect(update.install).not.toHaveBeenCalled();
    expect(update.close).toHaveBeenCalledTimes(1);
  });

  it("blocks restart for new drafts and allows retry after a restart error", async () => {
    const { state, relaunch, setBlock } = setup();
    await state.check();
    await state.install();
    setBlock("Send your new draft first");
    expect(await state.restart()).toBe(false);
    expect(relaunch).not.toHaveBeenCalled();
    setBlock("");
    relaunch.mockRejectedValueOnce(new Error("restart failed"));
    expect(await state.restart()).toBe(false);
    expect(state.phase).toBe("ready");
    expect(state.errorPhase).toBe("restart");
    expect(await state.restart()).toBe(true);
  });
});
