import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getAppState } from "./app-state.svelte";
import { savePersistedAppState } from "$lib/persistence/app-store";

type UpdateResource = Pick<Update, "version" | "body" | "download" | "install" | "close">;
type AvailableCallback = (version: string) => void | Promise<void>;
type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "downloaded"
  | "installing"
  | "ready"
  | "error";

interface UpdaterDependencies {
  check: () => Promise<UpdateResource | null>;
  relaunch: () => Promise<void>;
  blockedReason: () => string;
  prepareToExit: () => Promise<void>;
}

function defaultBlockedReason(): string {
  const app = getAppState();
  if (app.transferActive || app.receivingTransferActive)
    return "Wait for file and message transfers to finish before updating.";
  if (app.hasFiles || app.sendTextContent.trim())
    return "Send or clear your draft and attached files before updating.";
  return "";
}

export class UpdaterState {
  phase = $state<UpdatePhase>("idle");
  version = $state("");
  releaseNotes = $state("");
  error = $state("");
  errorPhase = $state<"check" | "install" | "restart" | null>(null);
  downloadedBytes = $state(0);
  totalBytes = $state<number | null>(null);
  detailsRequested = $state(false);
  hasChecked = $state(false);

  private resource: UpdateResource | null = null;
  private checkInFlight: Promise<void> | null = null;
  private installInFlight: Promise<boolean> | null = null;
  private restartInFlight: Promise<boolean> | null = null;
  private disposed = false;
  private deps: UpdaterDependencies;
  prepareToExit: () => Promise<void>;

  constructor(dependencies: Partial<UpdaterDependencies> = {}) {
    this.deps = {
      check: () => check({ timeout: 30_000 }),
      relaunch,
      blockedReason: defaultBlockedReason,
      prepareToExit: () => savePersistedAppState(getAppState().exportPersistedState()),
      ...dependencies,
    };
    this.prepareToExit = this.deps.prepareToExit;
  }

  get busy(): boolean {
    return this.phase === "checking" || this.phase === "downloading" || this.phase === "installing";
  }

  get blockedReason(): string {
    return this.deps.blockedReason();
  }

  get progressPercent(): number | null {
    return this.totalBytes && this.totalBytes > 0
      ? Math.min(100, Math.round((this.downloadedBytes / this.totalBytes) * 100))
      : null;
  }

  openDetails() {
    this.detailsRequested = true;
  }

  /** Checking only retrieves release metadata; it never installs or restarts. */
  check(onAvailable?: AvailableCallback): Promise<void> {
    if (
      this.disposed ||
      this.installInFlight ||
      this.phase === "downloaded" ||
      this.phase === "ready"
    )
      return Promise.resolve();
    if (this.checkInFlight) return this.checkInFlight;
    this.checkInFlight = this.performCheck(onAvailable).finally(() => {
      this.checkInFlight = null;
    });
    return this.checkInFlight;
  }

  private async closeResource(resource: UpdateResource | null) {
    if (resource) await resource.close().catch(() => {});
  }

  private async performCheck(onAvailable?: AvailableCallback) {
    this.phase = "checking";
    this.error = "";
    this.errorPhase = null;
    const previous = this.resource;
    this.resource = null;
    await this.closeResource(previous);
    if (this.disposed) return;
    try {
      const update = await this.deps.check();
      if (this.disposed) {
        await this.closeResource(update);
        return;
      }
      this.resource = update;
      this.hasChecked = true;
      this.version = update?.version ?? "";
      this.releaseNotes = update?.body ?? "";
      this.phase = update ? "available" : "idle";
      if (update && onAvailable) {
        // Notification failures do not turn a successful release check into a failed one.
        void Promise.resolve()
          .then(() => onAvailable(update.version))
          .catch(() => {});
      }
    } catch (error) {
      if (this.disposed) return;
      this.phase = "error";
      this.errorPhase = "check";
      this.error = `Could not check for updates: ${String(error)}`;
    }
  }

  /** Call only following the user's Update now / Retry update action. */
  install(): Promise<boolean> {
    if (this.installInFlight) return this.installInFlight;
    if (this.disposed || this.checkInFlight || this.phase === "ready")
      return Promise.resolve(false);
    if (this.blockedReason) {
      this.error = this.blockedReason;
      return Promise.resolve(false);
    }
    this.installInFlight = this.performInstall().finally(() => {
      this.installInFlight = null;
    });
    return this.installInFlight;
  }

  private async performInstall(): Promise<boolean> {
    this.error = "";
    this.errorPhase = null;
    try {
      if (!this.resource) await this.performCheck();
      const update = this.resource;
      if (!update || this.disposed) return false;
      if (this.phase !== "downloaded") {
        this.phase = "downloading";
        this.downloadedBytes = 0;
        this.totalBytes = null;
        await update.download(
          (event: DownloadEvent) => {
            if (this.disposed) return;
            if (event.event === "Started") {
              this.downloadedBytes = 0;
              this.totalBytes = event.data.contentLength ?? null;
            } else if (event.event === "Progress") {
              this.downloadedBytes += event.data.chunkLength;
            }
          },
          { timeout: 120_000 },
        );
      }
      if (this.disposed) return false;
      this.phase = "downloaded";
      if (this.blockedReason) {
        this.error = this.blockedReason;
        return false;
      }
      this.phase = "installing";
      await this.prepareToExit();
      if (this.disposed) return false;
      if (this.blockedReason) {
        this.phase = "downloaded";
        this.error = this.blockedReason;
        return false;
      }
      // Windows exits here to run the installer; this action is always explicit.
      await update.install({ restartAfterInstall: true });
      if (this.disposed) return false;
      this.phase = "ready";
      this.resource = null;
      await this.closeResource(update);
      return true;
    } catch (error) {
      if (!this.disposed) {
        this.phase = "error";
        this.errorPhase = "install";
        this.error = `Could not install the update: ${String(error)}`;
      }
      const failed = this.resource;
      this.resource = null;
      await this.closeResource(failed);
      return false;
    } finally {
      if (this.disposed) {
        const abandoned = this.resource;
        this.resource = null;
        await this.closeResource(abandoned);
      }
    }
  }

  restart(): Promise<boolean> {
    if (this.restartInFlight) return this.restartInFlight;
    if (this.disposed || this.phase !== "ready") return Promise.resolve(false);
    if (this.blockedReason) {
      this.error = this.blockedReason;
      return Promise.resolve(false);
    }
    this.restartInFlight = this.performRestart().finally(() => {
      this.restartInFlight = null;
    });
    return this.restartInFlight;
  }

  private async performRestart(): Promise<boolean> {
    this.error = "";
    try {
      await this.prepareToExit();
      if (this.disposed) return false;
      if (this.blockedReason) {
        this.error = this.blockedReason;
        return false;
      }
      await this.deps.relaunch();
      return true;
    } catch (error) {
      this.errorPhase = "restart";
      this.error = `Could not restart LanDrop: ${String(error)}`;
      return false;
    }
  }

  async dispose() {
    this.disposed = true;
    // In-flight operations release their resources after their native calls settle.
    if (this.installInFlight) return;
    const resource = this.resource;
    this.resource = null;
    await this.closeResource(resource);
  }
}

let instance: UpdaterState | null = null;

export function getUpdaterState(): UpdaterState {
  if (!instance) instance = new UpdaterState();
  return instance;
}
