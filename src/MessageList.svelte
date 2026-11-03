<script lang="ts">
  import { cleanSnippet, formatListDate, parseAddress, type Summary } from "./api";
  import Icon from "./Icon.svelte";
  import { store } from "./store.svelte";

  /** Two-line rows for the narrow list next to the reading pane. */
  let { compact = false }: { compact?: boolean } = $props();

  let rowsEl = $state<HTMLDivElement>();

  const showRecipient = $derived(
    store.view.kind === "label" && (store.view.id === "SENT" || store.view.id === "DRAFT"),
  );
  const title = $derived(
    store.view.kind === "search"
      ? store.view.q
        ? `「${store.view.q}」の検索結果`
        : "すべてのメール"
      : store.labelName(store.view.id),
  );
  const shown = $derived(store.messages.length);
  /** Older batches load once the list is scrolled within this distance of its end. */
  const LOAD_AHEAD_PX = 600;

  function checkLoadMore() {
    const el = rowsEl;
    if (el && el.scrollTop + el.clientHeight >= el.scrollHeight - LOAD_AHEAD_PX) store.loadMore();
  }

  // A batch that doesn't fill the window gives no scroll to wait for.
  $effect(() => {
    void store.messages.length;
    void store.nextToken;
    checkLoadMore();
  });

  function who(m: Summary): string {
    if (showRecipient) {
      const to = m.to ? parseAddress(m.to).name : "（宛先なし）";
      return `To: ${to}`;
    }
    const from = parseAddress(m.from);
    return from.address === store.current ? "自分" : from.name;
  }

  // User labels shown as chips before the subject, like Gmail.
  function chips(m: Summary) {
    return m.label_ids
      .filter((id) => id.startsWith("Label_") && !(store.view.kind === "label" && store.view.id === id))
      .map((id) => store.currentLabels.find((l) => l.id === id))
      .filter((l) => !!l)
      .slice(0, 3);
  }

  // Keep the keyboard cursor row in view.
  $effect(() => {
    const i = store.cursor;
    rowsEl?.querySelectorAll<HTMLElement>(".row")[i]?.scrollIntoView({ block: "nearest" });
  });

  function act(ev: MouseEvent, fn: () => void) {
    ev.stopPropagation();
    fn();
  }
</script>

<div class="list">
  <div class="toolbar">
    <button class="icon-btn" title="更新" onclick={() => store.syncAll()} disabled={store.loading}>
      <Icon name="refresh" />
    </button>
    <span class="title">{title}</span>
    {#if store.loading}<span class="loading">更新中…</span>{/if}
    <div class="spacer"></div>
    {#if shown}
      <span class="range">
        1–{shown.toLocaleString()} / {store.estimate > shown
          ? store.estimate.toLocaleString()
          : shown.toLocaleString()}{store.nextToken && store.estimate <= shown ? "+" : ""}
      </span>
    {/if}
  </div>

  {#if store.error}<div class="error-banner">{store.error}</div>{/if}

  <div class="rows" class:compact bind:this={rowsEl} onscroll={checkLoadMore}>
    {#each store.messages as m, i (m.id)}
      {@const unread = m.label_ids.includes("UNREAD")}
      {@const starred = m.label_ids.includes("STARRED")}
      <div
        class="row"
        class:unread
        class:cursor={i === store.cursor}
        class:selected={m.id === store.openId}
        role="button"
        tabindex="-1"
        onclick={() => store.open(m)}
        onkeydown={() => {}}
      >
        <button
          class="star"
          class:on={starred}
          title={starred ? "スターを外す" : "スターを付ける"}
          onclick={(e) => act(e, () => store.toggleStar(m))}
        >
          <Icon name={starred ? "star" : "starBorder"} />
        </button>
        <span class="who">{who(m)}</span>
        <span class="text">
          {#each chips(m) as l (l!.id)}<span class="chip">{l!.name}</span>{/each}
          <span class="subject">{m.subject || "（件名なし）"}</span>
          {#if !compact}<span class="snippet"> - {cleanSnippet(m.snippet)}</span>{/if}
        </span>
        {#if compact}<span class="snippet line2">{cleanSnippet(m.snippet)}</span>{/if}
        <span class="date">{formatListDate(m.date)}</span>
        <span class="hover-actions">
          {#if m.label_ids.includes("INBOX")}
            <button class="icon-btn small" title="アーカイブ" onclick={(e) => act(e, () => store.archive(m))}>
              <Icon name="archive" />
            </button>
          {/if}
          {#if !m.label_ids.includes("TRASH")}
            <button class="icon-btn small" title="削除" onclick={(e) => act(e, () => store.trash(m))}>
              <Icon name="delete" />
            </button>
          {/if}
          <button
            class="icon-btn small"
            title={unread ? "既読にする" : "未読にする"}
            onclick={(e) => act(e, () => store.toggleUnread(m))}
          >
            <Icon name={unread ? "markRead" : "markUnread"} />
          </button>
        </span>
      </div>
    {:else}
      <div class="empty">{store.loading ? "" : "メールはありません"}</div>
    {/each}
    {#if store.loadingMore}<div class="more">読み込み中…</div>{/if}
  </div>
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    flex: 1;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 48px;
    padding: 0 8px 0 12px;
    flex-shrink: 0;
  }
  .title {
    font-size: 14px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-left: 4px;
  }
  .loading {
    font-size: 12px;
    color: var(--text-3);
    margin-left: 8px;
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .range {
    font-size: 12px;
    color: var(--text-2);
    margin-right: 8px;
    white-space: nowrap;
  }
  .rows {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border-top: 1px solid var(--divider);
  }

  /* One-line row: star | sender | subject - snippet | date */
  .row {
    display: grid;
    grid-template-columns: 32px 200px minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    height: 40px;
    padding: 0 16px 0 12px;
    border-bottom: 1px solid var(--divider);
    background: var(--row-read);
    color: var(--text);
    cursor: pointer;
    position: relative;
  }
  .row.unread {
    background: var(--row-unread);
  }
  .row:hover {
    box-shadow: var(--row-shadow);
    z-index: 1;
  }
  .row.selected {
    background: var(--row-selected);
  }
  .row.cursor::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
    background: var(--text-2);
  }
  .star {
    display: flex;
    color: var(--text-3);
    opacity: 0.6;
  }
  .star:hover {
    opacity: 1;
  }
  .star.on {
    color: var(--star);
    opacity: 1;
  }
  .who,
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .unread .who,
  .unread .subject,
  .unread .date {
    font-weight: 700;
  }
  .snippet {
    color: var(--text-3);
  }
  .chip {
    display: inline-block;
    padding: 0 6px;
    margin-right: 6px;
    border-radius: 4px;
    background: var(--chip);
    color: var(--text-2);
    font-size: 12px;
    font-weight: 400;
    line-height: 18px;
    vertical-align: 1px;
  }
  .date {
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
    justify-self: end;
  }
  .unread .date {
    color: var(--text);
  }
  .hover-actions {
    display: none;
    position: absolute;
    right: 8px;
    top: 0;
    bottom: 0;
    align-items: center;
    gap: 2px;
    padding-left: 12px;
    background: inherit;
  }
  .row:hover .hover-actions {
    display: flex;
  }
  .row:hover .date {
    visibility: hidden;
  }

  /* Two-line row for the reading-pane layout. */
  .compact .row {
    grid-template-columns: 32px minmax(0, 1fr) auto;
    grid-template-rows: 22px 22px 20px;
    height: auto;
    padding-top: 8px;
    padding-bottom: 8px;
    row-gap: 0;
  }
  .compact .star {
    grid-row: 1 / 4;
    align-self: start;
  }
  .compact .who {
    grid-column: 2;
    grid-row: 1;
  }
  .compact .date {
    grid-column: 3;
    grid-row: 1;
  }
  .compact .text {
    grid-column: 2 / 4;
    grid-row: 2;
  }
  .compact .line2 {
    grid-column: 2 / 4;
    grid-row: 3;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .compact .hover-actions {
    top: auto;
    bottom: 4px;
    height: 32px;
  }
  .compact .row:hover .date {
    visibility: visible;
  }

  .empty {
    padding: 40px;
    text-align: center;
    color: var(--text-3);
  }
  .more {
    padding: 12px;
    text-align: center;
    font-size: 12px;
    color: var(--text-3);
  }
</style>
