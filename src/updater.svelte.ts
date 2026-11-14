import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { errorText } from "./api";

/** First check shortly after start, then once a day. Not Gmail API calls: no quota is used. */
const FIRST_CHECK_DELAY_MS = 15_000;
const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

export type UpdatePhase = "idle" | "checking" | "uptodate" | "available" | "downloading" | "installing" | "error";

class Updater {
  version = $state("");
  phase = $state<UpdatePhase>("idle");
  /** Plain class instance holding a native resource: keep it out of Svelte's proxying. */
  update = $state.raw<Update | null>(null);
  /** Download progress in percent, or -1 when the size is unknown. */
  progress = $state(0);
  error = $state("");
  /** The version whose notice the user closed with "あとで" (until the next start). */
  dismissed = $state("");

  /** Whether the toast should be on screen. */
  get showNotice() {
    if (this.phase === "downloading" || this.phase === "installing") return true;
    return this.phase === "available" && !!this.update && this.update.version !== this.dismissed;
  }

  init() {
    getVersion()
      .then((v) => (this.version = v))
      .catch(() => {});
    const first = setTimeout(() => this.check(false), FIRST_CHECK_DELAY_MS);
    const timer = setInterval(() => this.check(false), CHECK_INTERVAL_MS);
    return () => {
      clearTimeout(first);
      clearInterval(timer);
    };
  }

  /**
   * Looks for a newer version. A background check stays quiet when it fails (offline, no
   * release yet); a manual one reports the error.
   */
  async check(manual: boolean) {
    if (["checking", "downloading", "installing"].includes(this.phase)) return;
    const before = this.phase;
    this.phase = "checking";
    this.error = "";
    try {
      const found = await check();
      this.update = found;
      this.phase = found ? "available" : "uptodate";
    } catch (e) {
      if (manual) {
        this.error = errorText(e);
        this.phase = "error";
      } else {
        this.phase = before === "available" ? "available" : "idle";
      }
    }
  }

  /** Downloads, verifies the signature, installs, and restarts the app. */
  async install() {
    const update = this.update;
    if (!update || this.phase === "downloading" || this.phase === "installing") return;
    this.phase = "downloading";
    this.progress = 0;
    this.error = "";
    let total = 0;
    let received = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          this.progress = total ? 0 : -1;
        } else if (event.event === "Progress") {
          received += event.data.chunkLength;
          if (total) this.progress = Math.min(100, Math.round((received / total) * 100));
        } else if (event.event === "Finished") {
          this.phase = "installing";
        }
      });
      this.phase = "installing";
      // The Windows installer closes the app by itself; this covers the other platforms.
      await relaunch();
    } catch (e) {
      this.error = errorText(e);
      this.phase = "error";
    }
  }

  dismiss() {
    if (this.update) this.dismissed = this.update.version;
  }
}

export const updater = new Updater();
