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

class Store {
  info = $state<AppInfo | null>(null);
  showSettings = $state(false);
  current = $state("");
  labels = $state<Record<string, Label[]>>({});
  view = $state<View>(INBOX);
  messages = $state<Summary[]>([]);
  /** Fetches the next (older) batch; undefined once the whole view is loaded. */
  nextToken = $state<string | undefined>(undefined);
  /** Whether batches after the first have been appended. */
  private extended = false;
  estimate = $state(0);
  openId = $state<string | null>(null);
  /** Row highlighted for keyboard navigation. */
  cursor = $state(0);
  loading = $state(false);
  loadingMore = $state(false);
  error = $state("");
  searchText = $state("");
  splitPane = $state(loadPref("splitPane", false));
  sidebarCollapsed = $state(loadPref("sidebarCollapsed", false));

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

  /** Shows the cached first batch at once, then the server's. */
  async openView(v: View) {
    this.view = v;
    this.showSettings = false;
    this.openId = null;
    this.cursor = 0;
    this.nextToken = undefined;
    this.extended = false;
    this.estimate = v.kind === "label" ? (this.currentLabels.find((l) => l.id === v.id)?.total ?? 0) : 0;
    this.messages = await api.cachedMessages(this.current, v).catch(() => []);
    await this.refreshTop();
  }

  private isCurrent(email: string, key: string) {
    return email === this.current && key === viewKey(this.view);
  }

  /** Fetches the newest batch. Older batches already shown stay below it, refreshed from the cache. */
  async refreshTop() {
    const email = this.current;
    const key = viewKey(this.view);
    this.loading = true;
    try {
      const page = await api.refreshMessages(email, this.view);
      if (!this.isCurrent(email, key)) return;
      if (this.extended) {
        const top = new Set(page.messages.map((m) => m.id));
        const rest = this.messages.filter((m) => !top.has(m.id)).map((m) => m.id);
        const older = await api.cachedSummaries(email, rest);
        if (!this.isCurrent(email, key)) return;
        this.messages = [...page.messages, ...older.filter((m) => this.inView(m.label_ids))];
      } else {
        this.messages = page.messages;
        this.nextToken = page.next_page_token ?? undefined;
      }
      this.cursor = Math.min(this.cursor, Math.max(0, this.messages.length - 1));
      // Gmail's estimate is rough for labels; the label's own count is exact.
      const label = this.view.kind === "label" ? this.view.id : null;
      this.estimate = this.currentLabels.find((l) => l.id === label)?.total ?? page.estimate;
      this.error = "";
    } catch (e) {
      this.error = errorText(e);
    } finally {
      this.loading = false;
    }
  }

  /** Appends the next older batch, as the list scrolls near its end. */
  async loadMore() {
    const token = this.nextToken;
    if (!token || this.loadingMore) return;
    const email = this.current;
    const key = viewKey(this.view);
    this.loadingMore = true;
    try {
      const page = await api.refreshMessages(email, this.view, token);
      if (!this.isCurrent(email, key) || token !== this.nextToken) return;
      const have = new Set(this.messages.map((m) => m.id));
      this.messages = [...this.messages, ...page.messages.filter((m) => !have.has(m.id))];
      this.nextToken = page.next_page_token ?? undefined;
      this.extended = true;
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
    if (this.current && !this.showSettings) await this.refreshTop();
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

  toggleSplitPane() {
    this.splitPane = !this.splitPane;
    savePref("splitPane", this.splitPane);
  }

  toggleSidebar() {
    this.sidebarCollapsed = !this.sidebarCollapsed;
    savePref("sidebarCollapsed", this.sidebarCollapsed);
  }
}

export const store = new Store();
