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
      case "n":
        store.nextPage();
        return handled();
      case "p":
        store.prevPage();
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
      case "f":
        store.reply(m);
        return handled();
    }
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
  <div class="app" class:collapsed={store.sidebarCollapsed}>
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
          title={store.splitPane ? "閲覧ウィンドウ：なし" : "閲覧ウィンドウ：右"}
          onclick={() => store.toggleSplitPane()}
        >
          <Icon name={store.splitPane ? "noSplit" : "splitPane"} size={24} />
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

      <main class="card">
        {#if store.showSettings}
          <div class="settings-scroll"><Setup /></div>
        {:else if store.splitPane}
          <div class="split">
            <MessageList compact />
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
    display: grid;
    grid-template-columns: minmax(340px, 40%) minmax(0, 1fr);
  }
  .pane {
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--divider);
  }
</style>
