<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    displayName,
    errorText,
    formatDate,
    gmailUrls,
    type AppInfo,
    type Label,
    type Summary,
    type View,
  } from "./api";
  import MessageView from "./MessageView.svelte";
  import Setup from "./Setup.svelte";

  /** Polls Gmail for changes made on other devices. */
  const SYNC_INTERVAL_MS = 60_000;
  const INBOX: View = { kind: "label", id: "INBOX" };
  const SYSTEM_NAMES: Record<string, string> = {
    INBOX: "受信トレイ",
    SENT: "送信済み",
    STARRED: "スター付き",
    IMPORTANT: "重要",
    DRAFT: "下書き",
    SPAM: "迷惑メール",
    TRASH: "ゴミ箱",
  };

  let info = $state<AppInfo | null>(null);
  let showSettings = $state(false);
  let current = $state("");
  let labels = $state<Record<string, Label[]>>({});
  let view = $state<View>(INBOX);
  let messages = $state<Summary[]>([]);
  let selected = $state<Summary | null>(null);
  let searchText = $state("");
  let syncing = $state(false);
  let error = $state("");
  let showAllLabels = $state(false);

  const account = $derived(info?.accounts.find((a) => a.email === current));
  const currentLabels = $derived(labels[current] ?? []);
  const labelName = (l: Label) => SYSTEM_NAMES[l.id] ?? l.name;
  const unreadOf = (email: string, id: string) =>
    labels[email]?.find((l) => l.id === id)?.unread ?? 0;
  const userLabels = $derived(
    currentLabels.filter((l) => l.kind === "user").sort((a, b) => a.name.localeCompare(b.name, "ja")),
  );
  const favorites = $derived(
    (account?.favorites ?? [])
      .map((id) => currentLabels.find((l) => l.id === id))
      .filter((l): l is Label => !!l),
  );
  const unreadLabels = $derived(
    userLabels.filter((l) => l.unread > 0 && !account?.favorites.includes(l.id)),
  );
  const viewKey = (v: View) => (v.kind === "label" ? `label:${v.id}` : `search:${v.q}`);
  const isSentView = $derived(view.kind === "label" && view.id === "SENT");

  async function loadInfo(select?: string) {
    info = await api.appInfo();
    if (!info.configured || info.accounts.length === 0) {
      showSettings = true;
      return;
    }
    if (select || !info.accounts.some((a) => a.email === current)) {
      await switchAccount(select ?? info.accounts[0].email);
    }
  }

  async function switchAccount(email: string) {
    current = email;
    selected = null;
    view = INBOX;
    searchText = "";
    labels[email] = await api.cachedLabels(email).catch(() => []);
    await openView(INBOX);
    refreshLabels(email);
  }

  /** Shows the cached list at once, then replaces it with the server's. */
  async function openView(v: View) {
    view = v;
    selected = null;
    const email = current;
    const key = viewKey(v);
    messages = await api.cachedMessages(email, v).catch(() => []);
    try {
      syncing = true;
      const fresh = await api.refreshMessages(email, v);
      if (current === email && viewKey(view) === key) messages = fresh;
      error = "";
    } catch (e) {
      error = errorText(e);
    } finally {
      syncing = false;
    }
  }

  async function refreshLabels(email: string) {
    try {
      labels[email] = await api.refreshLabels(email);
    } catch (e) {
      if (email === current) error = errorText(e);
    }
  }

  async function syncAll() {
    if (!info) return;
    // Labels first: it applies read/label changes from other devices to the cache.
    await Promise.all(info.accounts.map((a) => refreshLabels(a.email)));
    if (!current) return;
    const email = current;
    const key = viewKey(view);
    try {
      const fresh = await api.refreshMessages(email, view);
      if (current === email && viewKey(view) === key) {
        messages = fresh;
        if (selected) selected = fresh.find((m) => m.id === selected!.id) ?? selected;
      }
    } catch (e) {
      error = errorText(e);
    }
  }

  async function select(m: Summary) {
    selected = m;
    if (!m.label_ids.includes("UNREAD")) return;
    const email = current;
    try {
      const ids = await api.setUnread(email, m.id, false);
      m.label_ids = ids;
      messages = messages.map((x) => (x.id === m.id ? { ...x, label_ids: ids } : x));
      if (selected?.id === m.id) selected = { ...m, label_ids: ids };
      refreshLabels(email);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function markUnread(m: Summary) {
    const ids = await api.setUnread(current, m.id, true).catch((e) => {
      error = errorText(e);
      return null;
    });
    if (!ids) return;
    messages = messages.map((x) => (x.id === m.id ? { ...x, label_ids: ids } : x));
    selected = null;
    refreshLabels(current);
  }

  async function toggleFavorite(l: Label) {
    if (!account) return;
    const on = !account.favorites.includes(l.id);
    await api.setFavorite(current, l.id, on);
    info = await api.appInfo();
  }

  function search(ev: SubmitEvent) {
    ev.preventDefault();
    const q = searchText.trim();
    openView(q ? { kind: "search", q } : INBOX);
  }

  const isActive = (id: string) => view.kind === "label" && view.id === id;

  function onKey(ev: KeyboardEvent) {
    if (ev.target instanceof HTMLInputElement) return;
    if (ev.key === "j" || ev.key === "k") {
      const i = messages.findIndex((m) => m.id === selected?.id);
      const next = messages[ev.key === "j" ? i + 1 : Math.max(0, i - 1)];
      if (next) select(next);
    } else if (ev.key === "/") {
      ev.preventDefault();
      document.querySelector<HTMLInputElement>(".search input")?.focus();
    }
  }

  onMount(() => {
    loadInfo();
    const timer = setInterval(syncAll, SYNC_INTERVAL_MS);
    return () => clearInterval(timer);
  });
</script>

<svelte:window onkeydown={onKey} />

{#if !info}
  <div class="center">読み込み中…</div>
{:else if showSettings || !info.configured || info.accounts.length === 0}
  <Setup
    {info}
    onchange={(added) => loadInfo(added)}
    onclose={info.configured && info.accounts.length ? () => (showSettings = false) : undefined}
  />
{:else}
  <div class="layout">
    <nav class="rail">
      {#each info.accounts as a (a.email)}
        <button
          class="avatar"
          class:active={a.email === current}
          title={a.email}
          onclick={() => switchAccount(a.email)}
        >
          {a.email[0].toUpperCase()}
          {#if unreadOf(a.email, "INBOX") > 0}
            <span class="badge">{unreadOf(a.email, "INBOX") > 99 ? "99+" : unreadOf(a.email, "INBOX")}</span>
          {/if}
        </button>
      {/each}
      <div class="spacer"></div>
      <button class="rail-btn" title="設定・アカウント追加" onclick={() => (showSettings = true)}>⚙</button>
    </nav>

    <aside class="labels">
      <div class="account-name" title={current}>{current}</div>
      <button class="compose btn" onclick={() => api.openGmail(current, gmailUrls.compose())}>✎ 作成</button>

      {#snippet labelRow(l: Label, star = false)}
        <div class="label-row" class:active={isActive(l.id)}>
          <button class="label-btn" onclick={() => openView({ kind: "label", id: l.id })}>
            <span class="name">{labelName(l)}</span>
            {#if l.unread > 0}<span class="count">{l.unread}</span>{/if}
          </button>
          {#if star}
            <button
              class="star"
              class:on={account?.favorites.includes(l.id)}
              title="お気に入り"
              onclick={() => toggleFavorite(l)}>★</button
            >
          {/if}
        </div>
      {/snippet}

      {#each currentLabels.filter((l) => l.id === "INBOX") as l}{@render labelRow(l)}{/each}
      {#if favorites.length}
        <h3>お気に入り</h3>
        {#each favorites as l (l.id)}{@render labelRow(l, true)}{/each}
      {/if}
      {#if unreadLabels.length}
        <h3>未読あり</h3>
        {#each unreadLabels as l (l.id)}{@render labelRow(l, true)}{/each}
      {/if}
      <h3>その他</h3>
      {#each currentLabels.filter((l) => l.id === "SENT" || l.id === "STARRED") as l}{@render labelRow(l)}{/each}
      <button class="toggle" onclick={() => (showAllLabels = !showAllLabels)}>
        {showAllLabels ? "▾" : "▸"} すべてのラベル
      </button>
      {#if showAllLabels}
        {#each userLabels as l (l.id)}{@render labelRow(l, true)}{/each}
      {/if}
      <div class="spacer"></div>
      <button class="link" onclick={() => api.openGmail(current, gmailUrls.filters())}>振り分け設定（Gmail）</button>
      <button class="link" onclick={() => api.openGmail(current)}>Gmail を開く</button>
    </aside>

    <section class="list">
      <form class="search" onsubmit={search}>
        <input bind:value={searchText} placeholder="検索（Gmail と同じ書き方）  /" />
      </form>
      <div class="status">
        {#if view.kind === "search"}「{view.q}」の検索結果{/if}
        {#if syncing}<span class="sync">更新中…</span>{/if}
      </div>
      {#if error}<div class="error" title={error}>{error}</div>{/if}
      <div class="rows">
        {#each messages as m (m.id)}
          <button
            class="row"
            class:unread={m.label_ids.includes("UNREAD")}
            class:active={selected?.id === m.id}
            onclick={() => select(m)}
          >
            <div class="line1">
              <span class="from">{displayName(isSentView ? `To: ${m.to}` : m.from)}</span>
              <span class="date">{formatDate(m.date)}</span>
            </div>
            <div class="subject">{m.subject || "（件名なし）"}</div>
            <div class="snippet">{m.snippet}</div>
          </button>
        {:else}
          <div class="empty">{syncing ? "" : "メールはありません"}</div>
        {/each}
      </div>
    </section>

    <main class="reader">
      {#if selected}
        {#key selected.id}
          <MessageView email={current} message={selected} />
        {/key}
        <button class="unread-btn btn" onclick={() => selected && markUnread(selected)}>未読にする</button>
      {:else}
        <div class="center muted">メールを選んでください</div>
      {/if}
    </main>
  </div>
{/if}

<style>
  .layout {
    display: grid;
    grid-template-columns: 64px 220px 360px 1fr;
    height: 100%;
  }
  .center {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
  }
  .muted {
    color: var(--text-muted);
  }
  .spacer {
    flex: 1;
  }

  .rail {
    background: var(--bg-rail);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 12px 0;
    gap: 10px;
  }
  .avatar {
    position: relative;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    border: 2px solid transparent;
    background: #4b5563;
    color: #fff;
    font-weight: 600;
    font-size: 17px;
  }
  .avatar.active {
    border-color: #fff;
    background: var(--accent);
  }
  .badge {
    position: absolute;
    top: -6px;
    right: -8px;
    background: var(--badge);
    color: #fff;
    font-size: 11px;
    border-radius: 9px;
    padding: 1px 5px;
    min-width: 18px;
  }
  .rail-btn {
    background: none;
    border: none;
    color: #9ca3af;
    font-size: 20px;
  }

  .labels {
    background: var(--bg-side);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 12px 8px;
    overflow-y: auto;
  }
  .account-name {
    font-weight: 600;
    padding: 0 8px 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .compose {
    margin: 0 4px 10px;
  }
  h3 {
    font-size: 12px;
    color: var(--text-muted);
    margin: 14px 8px 4px;
    font-weight: 600;
  }
  .label-row {
    display: flex;
    align-items: center;
    border-radius: 6px;
  }
  .label-row:hover {
    background: var(--bg-hover);
  }
  .label-row.active {
    background: var(--bg-active);
  }
  .label-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    justify-content: space-between;
    background: none;
    border: none;
    padding: 5px 8px;
    text-align: left;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    font-weight: 600;
    margin-left: 6px;
  }
  .star {
    background: none;
    border: none;
    color: var(--text-muted);
    opacity: 0;
    padding: 0 6px;
  }
  .label-row:hover .star,
  .star.on {
    opacity: 1;
  }
  .star.on {
    color: #f59e0b;
  }
  .toggle,
  .link {
    background: none;
    border: none;
    text-align: left;
    padding: 5px 8px;
    color: var(--text-muted);
  }
  .toggle {
    margin-top: 10px;
  }
  .link:hover,
  .toggle:hover {
    color: var(--text);
  }

  .list {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    min-width: 0;
  }
  .search {
    padding: 10px;
    border-bottom: 1px solid var(--border);
  }
  .search input {
    width: 100%;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-side);
  }
  .status {
    font-size: 12px;
    color: var(--text-muted);
    padding: 4px 12px;
    display: flex;
    justify-content: space-between;
    min-height: 22px;
  }
  .error {
    font-size: 12px;
    color: var(--error-text);
    background: var(--error-bg);
    padding: 6px 12px;
    max-height: 60px;
    overflow: hidden;
  }
  .rows {
    overflow-y: auto;
    flex: 1;
  }
  .row {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    border-bottom: 1px solid var(--border);
    padding: 8px 12px;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.active {
    background: var(--bg-active);
  }
  .line1 {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  .from,
  .subject,
  .snippet {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .date {
    font-size: 12px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .subject {
    margin-top: 2px;
  }
  .snippet {
    font-size: 12px;
    color: var(--text-muted);
    margin-top: 2px;
  }
  .row.unread .from,
  .row.unread .subject {
    font-weight: 700;
  }
  .row.unread {
    border-left: 3px solid var(--accent);
    padding-left: 9px;
  }
  .empty {
    padding: 24px;
    text-align: center;
    color: var(--text-muted);
  }

  .reader {
    position: relative;
    min-width: 0;
    overflow: hidden;
  }
  .unread-btn {
    position: absolute;
    right: 18px;
    bottom: 14px;
    font-size: 12px;
  }
</style>
