<script lang="ts">
  import { onMount } from "svelte";
  import { avatarColor } from "./api";
  import Icon from "./Icon.svelte";
  import MessageList from "./MessageList.svelte";
  import MessageView from "./MessageView.svelte";
  import Setup from "./Setup.svelte";
  import Sidebar from "./Sidebar.svelte";
  import { store } from "./store.svelte";

  let searchInput = $state<HTMLInputElement>();

  onMount(() => store.init());

  // Gmail keyboard shortcuts.
  function onKey(ev: KeyboardEvent) {
    const target = ev.target as HTMLElement;
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
      if (ev.key === "Escape") target.blur();
      return;
    }
    if (ev.ctrlKey || ev.altKey || ev.metaKey || store.showSettings || store.needsSetup) return;
    // Enter on a focused button is that button's click.
    if (ev.key === "Enter" && target instanceof HTMLButtonElement) return;
    const m = store.openMessage ?? store.messages[store.cursor];
    const handled = () => ev.preventDefault();
    switch (ev.key) {
      case "j":
        store.step(1);
        return handled();
      case "k":
        store.step(-1);
        return handled();
      case "o":
      case "Enter":
        if (m) store.open(m);
        return handled();
      case "u":
        store.close();
        return handled();
      case "/":
        searchInput?.focus();
        return handled();
      case "c":
        store.compose();
        return handled();
    }
    if (!m) return;
    switch (ev.key) {
      case "e":
        store.archive(m);
        return handled();
      case "#":
        store.trash(m);
        return handled();
      case "s":
        store.toggleStar(m);
        return handled();
      case "U":
        store.toggleUnread(m);
        return handled();
      case "r":
        store.composeFrom(m, "reply");
        return handled();
      case "a":
        store.composeFrom(m, "reply_all");
        return handled();
      case "f":
        store.composeFrom(m, "forward");
        return handled();
    }
  }

  // Drag the border between the label column and the mail list.
  let resizing = $state(false);
  function startResize(ev: PointerEvent) {
    ev.preventDefault();
    const el = ev.currentTarget as HTMLElement;
    el.setPointerCapture(ev.pointerId);
    resizing = true;
    const startX = ev.clientX;
    const startW = store.sidebarWidth;
    const move = (e: PointerEvent) => store.setSidebarWidth(startW + e.clientX - startX);
    const up = () => {
      resizing = false;
      store.setSidebarWidth(store.sidebarWidth, true);
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }
  function resetResize() {
    store.setSidebarWidth(256, true);
  }

  // Drag the border between the mail list and the reading pane.
  let splitEl = $state<HTMLElement>();
  let splitResizing = $state(false);
  function startSplitResize(ev: PointerEvent) {
    if (!splitEl) return;
    ev.preventDefault();
    const el = ev.currentTarget as HTMLElement;
    el.setPointerCapture(ev.pointerId);
    splitResizing = true;
    const below = store.splitMode === "below";
    const rect = splitEl.getBoundingClientRect();
    // Leave room for the reading pane.
    const max = below ? rect.height - 200 : rect.width - 320;
    const apply = (e: PointerEvent, save = false) =>
      below
        ? store.setSplitListHeight(e.clientY - rect.top, max, save)
        : store.setSplitListWidth(e.clientX - rect.left, max, save);
    const move = (e: PointerEvent) => apply(e);
    const up = (e: PointerEvent) => {
      splitResizing = false;
      apply(e, true);
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }
  function resetSplitResize() {
    if (store.splitMode === "below") store.setSplitListHeight(320, 1600, true);
    else store.setSplitListWidth(420, 1600, true);
  }

  function submitSearch(ev: SubmitEvent) {
    ev.preventDefault();
    store.search(store.searchText);
    searchInput?.blur();
  }
</script>

<svelte:window onkeydown={onKey} />

{#if !store.info}
  <div class="center">読み込み中…</div>
{:else if store.needsSetup}
  <div class="setup-page"><Setup /></div>
{:else}
  <div
    class="app"
    class:collapsed={store.sidebarCollapsed}
    class:resizing
    style:--sidebar-w="{store.sidebarWidth}px"
  >
    <header class="topbar">
      <div class="brand">
        <button class="icon-btn" title="メインメニュー" onclick={() => store.toggleSidebar()}>
          <Icon name="menu" size={24} />
        </button>
        <span class="logo"><Icon name="allMail" size={28} /></span>
        <span class="product">mailglance</span>
      </div>

      <form class="search" onsubmit={submitSearch}>
        <button class="icon-btn" type="submit" title="検索"><Icon name="search" size={24} /></button>
        <input bind:this={searchInput} bind:value={store.searchText} placeholder="メールを検索" />
        {#if store.searchText || store.view.kind === "search"}
          <button class="icon-btn" type="button" title="検索をクリア" onclick={() => store.search("")}>
            <Icon name="close" size={24} />
          </button>
        {/if}
      </form>

      <div class="top-actions">
        <button
          class="icon-btn"
          title={store.splitMode === "off"
            ? "閲覧ウィンドウ：なし（クリックで右に表示）"
            : store.splitMode === "right"
              ? "閲覧ウィンドウ：右（クリックで下に表示）"
              : "閲覧ウィンドウ：下（クリックで非表示）"}
          onclick={() => store.toggleSplitPane()}
        >
          <Icon
            name={store.splitMode === "off" ? "noSplit" : store.splitMode === "right" ? "splitPane" : "splitBelow"}
            size={24}
          />
        </button>
        <button class="icon-btn" title="設定" onclick={() => (store.showSettings = !store.showSettings)}>
          <Icon name="settings" size={24} />
        </button>
        <button
          class="me"
          title={store.current}
          style:background={avatarColor(store.current)}
          onclick={() => (store.showSettings = true)}
        >
          {store.current[0]?.toUpperCase()}
        </button>
      </div>
    </header>

    <div class="main">
      <nav class="accounts" aria-label="アカウント">
        {#each store.info.accounts as a (a.email)}
          {@const n = store.unreadOf(a.email, "INBOX")}
          <button
            class="account"
            class:active={a.email === store.current}
            title={a.email}
            onclick={() => store.switchAccount(a.email)}
          >
            <span class="avatar" style:background={avatarColor(a.email)}>{a.email[0].toUpperCase()}</span>
            {#if n > 0}<span class="badge">{n > 999 ? "999+" : n}</span>{/if}
          </button>
        {/each}
        <button class="icon-btn add" title="アカウントを追加" onclick={() => (store.showSettings = true)}>
          <Icon name="add" size={24} />
        </button>
      </nav>

      <Sidebar />
      {#if !store.sidebarCollapsed}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="resizer"
          title="ドラッグで幅を変更（ダブルクリックで元に戻す）"
          onpointerdown={startResize}
          ondblclick={resetResize}
        ></div>
      {/if}

      <main class="card">
        {#if store.showSettings}
          <div class="settings-scroll"><Setup /></div>
        {:else if store.splitPane}
          <div
            class="split {store.splitMode}"
            class:resizing={splitResizing}
            bind:this={splitEl}
            style:--list-w="{store.splitListWidth}px"
            style:--list-h="{store.splitListHeight}px"
          >
            <MessageList compact={store.splitMode === "right"} />
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="split-resizer"
              title="ドラッグで境界を移動（ダブルクリックで元に戻す）"
              onpointerdown={startSplitResize}
              ondblclick={resetSplitResize}
            ></div>
            <div class="pane">
              {#if store.openMessage}
                {#key store.openMessage.id}<MessageView message={store.openMessage} />{/key}
              {:else}
                <div class="center muted">メールが選択されていません</div>
              {/if}
            </div>
          </div>
        {:else if store.openMessage}
          {#key store.openMessage.id}<MessageView message={store.openMessage} showBack />{/key}
        {:else}
          <MessageList />
        {/if}
      </main>
    </div>
  </div>
{/if}

<style>
  .center {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
  }
  .muted {
    color: var(--text-3);
  }
  .setup-page {
    height: 100%;
    overflow-y: auto;
  }

  /* Every column gets min-height: 0 so its own content scrolls instead of the page. */
  .app {
    display: grid;
    grid-template-rows: 64px minmax(0, 1fr);
    height: 100%;
  }

  .topbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 4px;
  }
  .brand {
    display: flex;
    align-items: center;
    width: 300px;
    flex-shrink: 0;
    padding-left: 12px;
  }
  .collapsed .brand {
    width: auto;
  }
  .logo {
    display: flex;
    color: var(--accent);
    margin: 0 6px 0 8px;
  }
  .product {
    font-size: 22px;
    color: var(--text-2);
    letter-spacing: -0.2px;
  }
  .collapsed .product {
    margin-right: 16px;
  }
  .search {
    flex: 1;
    max-width: 720px;
    height: 48px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 4px;
    border-radius: 24px;
    background: var(--search);
  }
  .search:focus-within {
    background: var(--search-focus);
    box-shadow: 0 1px 1px 0 rgba(65, 69, 73, 0.3), 0 1px 3px 1px rgba(65, 69, 73, 0.15);
  }
  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: none;
    background: none;
    outline: none;
    font-size: 16px;
  }
  .search input::placeholder {
    color: var(--text-2);
  }
  .top-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .me {
    width: 32px;
    height: 32px;
    margin: 0 8px;
    border-radius: 50%;
    color: #fff;
    font-weight: 500;
  }

  .main {
    display: grid;
    grid-template-columns: 64px auto minmax(0, 1fr);
    min-height: 0;
    position: relative;
  }
  .resizer {
    position: absolute;
    top: 0;
    bottom: 16px;
    left: calc(64px + var(--sidebar-w) - 12px);
    width: 12px;
    cursor: col-resize;
    z-index: 5;
    touch-action: none;
  }
  .resizer:hover,
  .resizing .resizer {
    background: linear-gradient(to right, transparent 5px, var(--accent) 5px, var(--accent) 7px, transparent 7px);
  }
  .resizing {
    user-select: none;
    cursor: col-resize;
  }

  .accounts {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding-top: 12px;
    overflow-y: auto;
    min-height: 0;
  }
  .account {
    position: relative;
    padding: 3px;
    border-radius: 50%;
    border: 2px solid transparent;
  }
  .account.active {
    border-color: var(--accent);
  }
  .avatar {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-weight: 500;
    font-size: 16px;
  }
  .badge {
    position: absolute;
    top: -4px;
    right: -10px;
    min-width: 20px;
    padding: 1px 5px;
    border-radius: 10px;
    background: var(--badge);
    color: #fff;
    font-size: 11px;
    font-weight: 600;
    border: 2px solid var(--page);
  }

  .card {
    background: var(--card);
    border-radius: 16px;
    margin: 0 16px 16px 0;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .settings-scroll {
    overflow-y: auto;
    min-height: 0;
    flex: 1;
  }
  .split {
    flex: 1;
    min-height: 0;
    position: relative;
    display: grid;
  }
  .split.right {
    grid-template-columns: var(--list-w) minmax(0, 1fr);
  }
  .split.below {
    grid-template-rows: var(--list-h) minmax(0, 1fr);
  }
  .split.resizing {
    user-select: none;
  }
  .split.right.resizing {
    cursor: col-resize;
  }
  .split.below.resizing {
    cursor: row-resize;
  }
  .pane {
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .right .pane {
    border-left: 1px solid var(--divider);
  }
  .below .pane {
    border-top: 1px solid var(--divider);
  }
  .split-resizer {
    position: absolute;
    z-index: 5;
    touch-action: none;
  }
  .right > .split-resizer {
    top: 0;
    bottom: 0;
    left: calc(var(--list-w) - 5px);
    width: 10px;
    cursor: col-resize;
  }
  .below > .split-resizer {
    left: 0;
    right: 0;
    top: calc(var(--list-h) - 5px);
    height: 10px;
    cursor: row-resize;
  }
  .right > .split-resizer:hover,
  .resizing.right > .split-resizer {
    background: linear-gradient(to right, transparent 4px, var(--accent) 4px, var(--accent) 6px, transparent 6px);
  }
  .below > .split-resizer:hover,
  .resizing.below > .split-resizer {
    background: linear-gradient(to bottom, transparent 4px, var(--accent) 4px, var(--accent) 6px, transparent 6px);
  }
</style>
