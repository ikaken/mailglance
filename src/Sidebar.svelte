<script lang="ts">
  import type { Label, View } from "./api";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";
  import { store, SYSTEM_NAMES } from "./store.svelte";

  let showMore = $state(false);
  let showAllLabels = $state(false);

  const collapsed = $derived(store.sidebarCollapsed);
  const favorites = $derived(store.account?.favorites ?? []);
  const userLabels = $derived(
    store.currentLabels
      .filter((l) => l.kind === "user")
      .sort((a, b) => a.name.localeCompare(b.name, "ja")),
  );
  const pinned = $derived(
    favorites.map((id) => userLabels.find((l) => l.id === id)).filter((l): l is Label => !!l),
  );
  const unreadLabels = $derived(userLabels.filter((l) => l.unread > 0 && !favorites.includes(l.id)));
  const otherLabels = $derived(
    userLabels.filter((l) => !favorites.includes(l.id) && !unreadLabels.includes(l)),
  );

  // Gmail's order; the rest sit under "もっと見る".
  const main: [string, IconName][] = [
    ["INBOX", "inbox"],
    ["STARRED", "star"],
    ["SENT", "send"],
    ["DRAFT", "draft"],
  ];
  const more: [string, IconName][] = [
    ["IMPORTANT", "important"],
    ["SPAM", "spam"],
    ["TRASH", "delete"],
  ];

  const ALL_MAIL: View = { kind: "search", q: "" };
  const isActive = (id: string) => store.view.kind === "label" && store.view.id === id;
  const isAllMail = $derived(store.view.kind === "search" && store.view.q === "");
  const unread = (id: string) => store.currentLabels.find((l) => l.id === id)?.unread ?? 0;
  // Gmail shows no unread count for these.
  const NO_COUNT = ["SENT", "DRAFT", "STARRED", "IMPORTANT", "TRASH"];

  function go(id: string) {
    store.searchText = "";
    store.openView({ kind: "label", id });
  }
</script>

{#snippet item(id: string, icon: IconName, name: string, color: string | null = null, pinnable = false)}
  {@const count = NO_COUNT.includes(id) ? 0 : unread(id)}
  <div class="item" class:active={isActive(id)} class:bold={count > 0}>
    <button class="item-btn" title={name} onclick={() => go(id)}>
      <span class="icon" style:color={color}><Icon name={icon} /></span>
      {#if !collapsed}
        <span class="name">{name}</span>
        {#if count > 0}<span class="count">{count.toLocaleString()}</span>{/if}
      {:else if count > 0}
        <span class="dot"></span>
      {/if}
    </button>
    {#if pinnable && !collapsed}
      {@const on = favorites.includes(id)}
      <button
        class="pin"
        class:on
        title={on ? "お気に入りから外す" : "お気に入りに追加"}
        onclick={() => {
          const l = userLabels.find((x) => x.id === id);
          if (l) store.toggleFavorite(l);
        }}><Icon name="pin" size={16} /></button
      >
    {/if}
  </div>
{/snippet}

{#snippet labelItem(l: Label)}
  {@render item(l.id, "label", l.name, l.color, true)}
{/snippet}

<aside class="sidebar" class:collapsed>
  <button class="compose" onclick={() => store.compose()} title="作成（Gmail で開く）">
    <Icon name="edit" size={24} />
    {#if !collapsed}<span>作成</span>{/if}
  </button>

  <nav>
    {#each main as [id, icon]}{@render item(id, icon, SYSTEM_NAMES[id])}{/each}
    {#if showMore}
      {#each more as [id, icon]}{@render item(id, icon, SYSTEM_NAMES[id])}{/each}
      <div class="item" class:active={isAllMail}>
        <button class="item-btn" title="すべてのメール" onclick={() => store.openView(ALL_MAIL)}>
          <span class="icon"><Icon name="allMail" /></span>
          {#if !collapsed}<span class="name">すべてのメール</span>{/if}
        </button>
      </div>
    {/if}
    <div class="item">
      <button class="item-btn" onclick={() => (showMore = !showMore)}>
        <span class="icon"><Icon name={showMore ? "expandLess" : "expandMore"} /></span>
        {#if !collapsed}<span class="name">{showMore ? "折りたたむ" : "もっと見る"}</span>{/if}
      </button>
    </div>

    {#if !collapsed}<h3>ラベル</h3>{/if}
    {#each pinned as l (l.id)}{@render labelItem(l)}{/each}
    {#each unreadLabels as l (l.id)}{@render labelItem(l)}{/each}
    {#if showAllLabels}
      {#each otherLabels as l (l.id)}{@render labelItem(l)}{/each}
    {/if}
    {#if otherLabels.length}
      <div class="item">
        <button class="item-btn" onclick={() => (showAllLabels = !showAllLabels)}>
          <span class="icon"><Icon name={showAllLabels ? "expandLess" : "expandMore"} /></span>
          {#if !collapsed}
            <span class="name">{showAllLabels ? "折りたたむ" : `すべてのラベル（${otherLabels.length}）`}</span>
          {/if}
        </button>
      </div>
    {/if}
  </nav>
</aside>

<style>
  .sidebar {
    width: var(--sidebar-w, 256px);
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding-right: 16px;
  }
  .sidebar.collapsed {
    width: 72px;
    padding-right: 0;
  }
  .compose {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 56px;
    min-width: 56px;
    margin: 8px 0 16px 8px;
    padding: 0 24px 0 16px;
    align-self: flex-start;
    border-radius: 16px;
    background: var(--compose);
    color: var(--compose-text);
    font-weight: 500;
    font-size: 14px;
    transition: box-shadow 0.08s;
  }
  .compose:hover {
    box-shadow: 0 1px 3px 0 rgba(60, 64, 67, 0.3), 0 4px 8px 3px rgba(60, 64, 67, 0.15);
  }
  .collapsed .compose {
    padding: 0 16px;
  }
  nav {
    overflow-y: auto;
    min-height: 0;
    flex: 1;
    padding-bottom: 16px;
  }
  h3 {
    font-size: 16px;
    font-weight: 500;
    margin: 16px 0 4px 26px;
    color: var(--text);
  }
  .item {
    display: flex;
    align-items: center;
    height: 32px;
    border-radius: 0 16px 16px 0;
    color: var(--text-2);
  }
  .collapsed .item {
    border-radius: 16px;
    margin-left: 12px;
    width: 48px;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.active {
    background: var(--nav-active);
    color: var(--nav-active-text);
    font-weight: 700;
  }
  .item.bold {
    font-weight: 700;
    color: var(--text);
  }
  .item-btn {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 0 12px 0 26px;
    text-align: left;
    position: relative;
  }
  .collapsed .item-btn {
    padding: 0 14px;
  }
  .icon {
    display: flex;
    color: inherit;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    font-size: 12px;
    letter-spacing: 0.2px;
  }
  .dot {
    position: absolute;
    top: 6px;
    right: 10px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--badge);
  }
  .pin {
    display: flex;
    padding: 0 10px;
    color: var(--text-3);
    opacity: 0;
  }
  .item:hover .pin,
  .pin.on {
    opacity: 1;
  }
  .pin.on {
    color: var(--accent);
  }
</style>
