import { api, errorText, gmailUrls, type AppInfo, type Label, type Summary, type View } from "./api";

/** Polls Gmail for changes made on other devices. */
const SYNC_INTERVAL_MS = 60_000;
export const INBOX: View = { kind: "label", id: "INBOX" };

export const SYSTEM_NAMES: Record<string, string> = {
  INBOX: "受信トレイ",
  STARRED: "スター付き",
  IMPORTANT: "重要",
  SENT: "送信済み",
  DRAFT: "下書き",
  SPAM: "迷惑メール",
  TRASH: "ゴミ箱",
};

export const viewKey = (v: View) => (v.kind === "label" ? `label:${v.id}` : `search:${v.q}`);

function loadPref<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(`mailglance.${key}`);
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}

function savePref(key: string, value: unknown) {
  try {
    localStorage.setItem(`mailglance.${key}`, JSON.stringify(value));
  } catch {
    // Preferences are a convenience; ignore storage failures.
  }
}

export type SplitMode = "off" | "right" | "below";

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, Math.round(n)));

function loadSplitMode(): SplitMode {
  const v = loadPref<unknown>("splitMode", null);
  if (v === "off" || v === "right" || v === "below") return v;
  // Older versions stored a boolean "splitPane" (true = right).
  return loadPref("splitPane", false) ? "right" : "off";
}

class Store {
  info = $state<AppInfo | null>(null);
  showSettings = $state(false);
  current = $state("");
  labels = $state<Record<string, Label[]>>({});
  view = $state<View>(INBOX);
  messages = $state<Summary[]>([]);
  /** Token for the next (older) chunk; undefined once the end is reached. */
  nextToken = $state<string | undefined>(undefined);
  /** How many chunks of PAGE_SIZE are loaded; the list grows as the user scrolls. */
  chunks = $state(0);
  loadingMore = $state(false);
  estimate = $state(0);
  openId = $state<string | null>(null);
  /** Row highlighted for keyboard navigation. */
  cursor = $state(0);
  loading = $state(false);
  error = $state("");
  searchText = $state("");
  /** Reading pane placement: off, to the right of the list, or below it. */
  splitMode = $state<SplitMode>(loadSplitMode());
  /** Width of the mail list with the pane on the right, height with it below (px). */
  splitListWidth = $state(clamp(Number(loadPref("splitListWidth", 420)) || 420, 280, 1600));
  splitListHeight = $state(clamp(Number(loadPref("splitListHeight", 320)) || 320, 120, 1600));
  sidebarCollapsed = $state(loadPref("sidebarCollapsed", false));
  sidebarWidth = $state(Math.min(480, Math.max(180, Number(loadPref("sidebarWidth", 256)) || 256)));

  get splitPane() {
    return this.splitMode !== "off";
  }

  get hasNext() {
    return !!this.nextToken;
  }

  get account() {
    return this.info?.accounts.find((a) => a.email === this.current);
  }

  get currentLabels(): Label[] {
    return this.labels[this.current] ?? [];
  }

  get openMessage(): Summary | null {
    return this.messages.find((m) => m.id === this.openId) ?? null;
  }

  get needsSetup() {
    return !this.info?.configured || this.info.accounts.length === 0;
  }

  labelName(id: string): string {
    return SYSTEM_NAMES[id] ?? this.currentLabels.find((l) => l.id === id)?.name ?? id;
  }

  unreadOf(email: string, id: string) {
    return this.labels[email]?.find((l) => l.id === id)?.unread ?? 0;
  }

  init() {
    this.loadInfo();
    const timer = setInterval(() => this.syncAll(), SYNC_INTERVAL_MS);
    return () => clearInterval(timer);
  }

  async loadInfo(select?: string) {
    this.info = await api.appInfo();
    if (this.needsSetup) {
      this.showSettings = true;
      return;
    }
    if (select || !this.info.accounts.some((a) => a.email === this.current)) {
      this.showSettings = false;
      await this.switchAccount(select ?? this.info.accounts[0].email);
    }
  }

  async switchAccount(email: string) {
    this.current = email;
    this.showSettings = false;
    this.labels[email] = await api.cachedLabels(email).catch(() => []);
    this.searchText = "";
    await this.openView(INBOX);
    this.refreshLabels(email);
  }

  /** Shows the cached first page at once, then the server's. */
  async openView(v: View) {
    this.view = v;
    this.showSettings = false;
    this.openId = null;
    this.cursor = 0;
    this.nextToken = undefined;
    this.chunks = 0;
    this.estimate = v.kind === "label" ? (this.currentLabels.find((l) => l.id === v.id)?.total ?? 0) : 0;
    this.messages = await api.cachedMessages(this.current, v).catch(() => []);
    await this.refreshHead();
  }

  private setEstimate(fallback: number) {
    // Gmail's estimate is rough for labels; the label's own count is exact.
    const label = this.view.kind === "label" ? this.view.id : null;
    this.estimate = this.currentLabels.find((l) => l.id === label)?.total ?? fallback;
  }

  /**
   * Fetches the newest chunk. Costs one list call (5 units) plus metadata only for messages not
   * cached yet, so the 60-second sync stays cheap however far the user has scrolled. Chunks
   * already loaded stay; their labels are re-read from the cache, which the history sync updates.
   */
  async refreshHead() {
    const email = this.current;
    const key = viewKey(this.view);
    const stale = () => email !== this.current || key !== viewKey(this.view);
    this.loading = true;
    try {
      const page = await api.refreshMessages(email, this.view, undefined);
      if (stale()) return;
      if (this.chunks <= 1) {
        this.messages = page.messages;
        this.nextToken = page.next_page_token ?? undefined;
        this.chunks = 1;
      } else {
        const head = new Set(page.messages.map((m) => m.id));
        const tail = this.messages.filter((m) => !head.has(m.id));
        const fresh = await api.cachedSummaries(email, tail.map((m) => m.id)).catch(() => tail);
        if (stale()) return;
        this.messages = [...page.messages, ...fresh].filter((m) => this.inView(m.label_ids));
      }
      this.setEstimate(page.estimate);
      this.cursor = Math.min(this.cursor, Math.max(0, this.messages.length - 1));
      this.error = "";
    } catch (e) {
      this.error = errorText(e);
    } finally {
      this.loading = false;
    }
  }

  /** Appends the next chunk. Called when the list is scrolled near its end. */
  async loadMore() {
    const token = this.nextToken;
    if (!token || this.loadingMore || this.loading) return;
    const email = this.current;
    const key = viewKey(this.view);
    this.loadingMore = true;
    try {
      const page = await api.refreshMessages(email, this.view, token);
      if (email !== this.current || key !== viewKey(this.view)) return;
      const have = new Set(this.messages.map((m) => m.id));
      this.messages = [...this.messages, ...page.messages.filter((m) => !have.has(m.id))];
      this.nextToken = page.next_page_token ?? undefined;
      this.chunks += 1;
      this.error = "";
    } catch (e) {
      this.error = errorText(e);
    } finally {
      this.loadingMore = false;
    }
  }

  async refreshLabels(email: string) {
    try {
      this.labels[email] = await api.refreshLabels(email);
    } catch (e) {
      if (email === this.current) this.error = errorText(e);
    }
  }

  async syncAll() {
    if (!this.info || this.needsSetup) return;
    // Labels first: it applies read/label changes from other devices to the cache.
    await Promise.all(this.info.accounts.map((a) => this.refreshLabels(a.email)));
    if (this.current && !this.showSettings && !this.loadingMore) await this.refreshHead();
  }

  search(q: string) {
    q = q.trim();
    this.searchText = q;
    this.openView(q ? { kind: "search", q } : INBOX);
  }

  async open(m: Summary) {
    this.openId = m.id;
    this.showSettings = false;
    const i = this.messages.findIndex((x) => x.id === m.id);
    if (i >= 0) this.cursor = i;
    if (m.label_ids.includes("UNREAD")) await this.modify(m, [], ["UNREAD"]);
  }

  close() {
    this.openId = null;
  }

  /** Opens the next (older) or previous (newer) message, as j/k do in Gmail. */
  step(delta: number) {
    const i = Math.min(Math.max(this.cursor + delta, 0), this.messages.length - 1);
    if (i < 0) return;
    this.cursor = i;
    if (this.openId || this.splitPane) this.open(this.messages[i]);
    if (i >= this.messages.length - 5) this.loadMore();
  }

  /** Applies label changes, keeping unread counts in step locally. */
  private applyLabels(m: Summary, after: string[]) {
    const before = m.label_ids;
    const email = this.current;
    const unread = (ids: string[], l: string) => ids.includes("UNREAD") && ids.includes(l);
    this.labels[email] = (this.labels[email] ?? []).map((l) => {
      const delta = Number(unread(after, l.id)) - Number(unread(before, l.id));
      return delta ? { ...l, unread: Math.max(0, l.unread + delta) } : l;
    });
    this.messages = this.messages.map((x) => (x.id === m.id ? { ...x, label_ids: after } : x));
  }

  async modify(m: Summary, add: string[], remove: string[]) {
    try {
      const after = await api.modifyMessage(this.current, m.id, add, remove);
      this.applyLabels(m, after);
      return after;
    } catch (e) {
      this.error = errorText(e);
      return null;
    }
  }

  toggleStar(m: Summary) {
    const on = m.label_ids.includes("STARRED");
    this.modify(m, on ? [] : ["STARRED"], on ? ["STARRED"] : []);
  }

  async toggleUnread(m: Summary) {
    const unread = m.label_ids.includes("UNREAD");
    await this.modify(m, unread ? [] : ["UNREAD"], unread ? ["UNREAD"] : []);
    // Gmail leaves the message after marking it unread.
    if (!unread && this.openId === m.id && !this.splitPane) this.close();
  }

  async archive(m: Summary) {
    const after = await this.modify(m, [], ["INBOX"]);
    if (after && !this.inView(after)) this.drop(m.id);
  }

  async trash(m: Summary) {
    try {
      const after = await api.trashMessage(this.current, m.id);
      this.applyLabels(m, after);
      if (!this.inView(after)) this.drop(m.id);
    } catch (e) {
      this.error = errorText(e);
    }
  }

  /** Whether a message with these labels still belongs to the current view. */
  private inView(labelIds: string[]) {
    if (this.view.kind === "search") return !labelIds.includes("TRASH");
    return labelIds.includes(this.view.id);
  }

  private drop(id: string) {
    const i = this.messages.findIndex((m) => m.id === id);
    if (i < 0) return;
    this.messages = this.messages.filter((m) => m.id !== id);
    this.cursor = Math.min(i, Math.max(0, this.messages.length - 1));
    if (this.openId === id) {
      // Split view moves on to the next message; full view goes back to the list, as Gmail does.
      if (this.splitPane && this.messages[this.cursor]) this.open(this.messages[this.cursor]);
      else this.openId = null;
    }
  }

  async toggleFavorite(label: Label) {
    if (!this.account) return;
    const on = !this.account.favorites.includes(label.id);
    await api.setFavorite(this.current, label.id, on);
    this.info = await api.appInfo();
  }

  compose() {
    api.openGmail(this.current, gmailUrls.compose());
  }

  reply(m: Summary) {
    api.openGmail(this.current, gmailUrls.thread(m.thread_id));
  }

  /** Cycles the reading pane: off → right → below → off. */
  toggleSplitPane() {
    this.splitMode = this.splitMode === "off" ? "right" : this.splitMode === "right" ? "below" : "off";
    savePref("splitMode", this.splitMode);
  }

  setSplitListWidth(w: number, max = 1600, save = false) {
    this.splitListWidth = clamp(w, 280, Math.max(280, max));
    if (save) savePref("splitListWidth", this.splitListWidth);
  }

  setSplitListHeight(h: number, max = 1600, save = false) {
    this.splitListHeight = clamp(h, 120, Math.max(120, max));
    if (save) savePref("splitListHeight", this.splitListHeight);
  }

  toggleSidebar() {
    this.sidebarCollapsed = !this.sidebarCollapsed;
    savePref("sidebarCollapsed", this.sidebarCollapsed);
  }

  setSidebarWidth(w: number, save = false) {
    this.sidebarWidth = Math.min(480, Math.max(180, Math.round(w)));
    if (save) savePref("sidebarWidth", this.sidebarWidth);
  }
}

export const store = new Store();
