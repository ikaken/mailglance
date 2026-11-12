<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    api,
    avatarColor,
    errorText,
    formatFullDate,
    gmailUrls,
    parseAddress,
    type MessageBody,
    type Summary,
  } from "./api";
  import Icon from "./Icon.svelte";
  import { store } from "./store.svelte";

  let { message, showBack = false }: { message: Summary; showBack?: boolean } = $props();

  let body = $state<MessageBody | null>(null);
  let error = $state("");
  let showImages = $state(false);
  let frame = $state<HTMLIFrameElement>();
  // Starts small: scrollHeight never reports less than the frame's own height.
  let frameHeight = $state(80);
  let resize: ResizeObserver | undefined;

  // The view is recreated per message ({#key}), so this runs once per message.
  const email = store.current;
  // svelte-ignore state_referenced_locally
  const id = message.id;
  api
    .getBody(email, id)
    .then((b) => (body = b))
    .catch((e) => (error = errorText(e)));

  onDestroy(() => resize?.disconnect());

  const from = $derived(parseAddress(message.from));
  const unread = $derived(message.label_ids.includes("UNREAD"));
  const starred = $derived(message.label_ids.includes("STARRED"));
  const chips = $derived(
    message.label_ids
      .filter((l) => l === "INBOX" || l.startsWith("Label_"))
      .map((l) => ({ id: l, name: store.labelName(l) })),
  );
  const hasRemoteImages = $derived(
    !!body && /(<img[^>]+src\s*=\s*["']?https?:|url\(\s*["']?https?:|background\s*=\s*["']?https?:)/i.test(body.html),
  );

  // Scripts never run (sandbox without allow-scripts). Remote loads are blocked by CSP
  // until the user asks for images, which also stops tracking pixels.
  const srcdoc = $derived.by(() => {
    if (!body) return "";
    const remote = showImages ? " https: http:" : "";
    const csp = `default-src 'none'; img-src data:${remote}; style-src 'unsafe-inline'${remote}; font-src data:${remote}; media-src data:${remote}`;
    return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<style>html,body{margin:0}body{padding:0 4px 16px;font-family:Arial,"Yu Gothic UI",Meiryo,sans-serif;font-size:14px;color:#222;background:#fff;overflow-wrap:anywhere}img{max-width:100%;height:auto}a{color:#1a73e8}</style>
</head><body>${body.html}</body></html>`;
  });

  // The frame grows to its content so the whole message scrolls as one page, like Gmail.
  // No margin is added: with `body{height:100%}` emails that would feed back into endless growth.
  function fit() {
    const h = frame?.contentDocument?.documentElement?.scrollHeight ?? 0;
    if (h && Math.abs(h - frameHeight) > 1) frameHeight = h;
  }

  // Every link goes to the app, which hands it to the last active Chrome window.
  function onFrameLoad() {
    const doc = frame?.contentDocument;
    if (!doc) return;
    const handler = (ev: MouseEvent) => {
      const a = (ev.target as Element | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
      if (!a) return;
      ev.preventDefault();
      const href = (a.getAttribute("href") ?? "").trim();
      if (/^(https?:|mailto:)/i.test(href)) {
        api.openLink(email, href).catch((e) => (error = errorText(e)));
      }
    };
    doc.addEventListener("click", handler);
    doc.addEventListener("auxclick", handler);
    // Forms are already blocked by the sandbox; this keeps them from showing an error page.
    doc.addEventListener("submit", (ev) => ev.preventDefault());
    // Images finishing late change the height.
    doc.addEventListener("load", fit, true);
    resize?.disconnect();
    resize = new ResizeObserver(fit);
    if (doc.body) resize.observe(doc.body);
    fit();
  }

  function formatSize(n: number) {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }
</script>

<div class="view">
  <div class="toolbar">
    {#if showBack}
      <button class="icon-btn" title="戻る（u）" onclick={() => store.close()}><Icon name="arrowBack" /></button>
    {/if}
    {#if message.label_ids.includes("INBOX")}
      <button class="icon-btn" title="アーカイブ（e）" onclick={() => store.archive(message)}>
        <Icon name="archive" />
      </button>
    {/if}
    {#if !message.label_ids.includes("TRASH")}
      <button class="icon-btn" title="削除（#）" onclick={() => store.trash(message)}><Icon name="delete" /></button>
    {/if}
    <button
      class="icon-btn"
      title={unread ? "既読にする" : "未読にする（Shift+U）"}
      onclick={() => store.toggleUnread(message)}
    >
      <Icon name={unread ? "markRead" : "markUnread"} />
    </button>
    <div class="spacer"></div>
    <button class="icon-btn" title="Gmail で開く" onclick={() => store.openInGmail(message)}>
      <Icon name="openInNew" />
    </button>
  </div>

  <div class="scroll">
    <div class="subject-row">
      <h2>{message.subject || "（件名なし）"}</h2>
      {#each chips as c (c.id)}<span class="chip">{c.name}</span>{/each}
    </div>

    <div class="sender">
      <span class="avatar" style:background={avatarColor(from.address)}>{from.name[0]?.toUpperCase()}</span>
      <div class="who">
        <div>
          <span class="name">{from.name}</span>
          {#if from.address !== from.name}<span class="addr">&lt;{from.address}&gt;</span>{/if}
        </div>
        {#if message.to}<div class="to">To: {message.to}</div>{/if}
      </div>
      <span class="date">{formatFullDate(message.date)}</span>
      <button
        class="icon-btn star"
        class:on={starred}
        title={starred ? "スターを外す（s）" : "スターを付ける（s）"}
        onclick={() => store.toggleStar(message)}
      >
        <Icon name={starred ? "star" : "starBorder"} />
      </button>
      <button
        class="icon-btn"
        title="全員に返信（a）"
        disabled={store.composing}
        onclick={() => store.composeFrom(message, "reply_all")}
      >
        <Icon name="replyAll" />
      </button>
    </div>

    {#if hasRemoteImages && !showImages}
      <div class="images-banner">
        画像は表示されていません。
        <button class="link" onclick={() => (showImages = true)}>下に画像を表示</button>
      </div>
    {/if}

    {#if error}
      <div class="error-banner">{error}</div>
    {:else if !body}
      <p class="loading">読み込み中…</p>
    {:else}
      <iframe
        bind:this={frame}
        title="本文"
        sandbox="allow-same-origin"
        {srcdoc}
        style:height="{frameHeight}px"
        onload={onFrameLoad}
      ></iframe>

      {#if body.attachments.length}
        <div class="attachments">
          <div class="att-title">添付ファイル {body.attachments.length} 件（保存は Gmail の画面から）</div>
          <div class="att-list">
            {#each body.attachments as a}
              <button class="att" title="Gmail で開く" onclick={() => store.openInGmail(message)}>
                <Icon name="attach" size={18} />
                <span class="att-name">{a.filename}</span>
                <span class="att-size">{formatSize(a.size)}</span>
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <div class="actions">
        <button class="pill-btn" disabled={store.composing} onclick={() => store.composeFrom(message, "reply")}>
          <Icon name="reply" />返信
        </button>
        <button class="pill-btn" disabled={store.composing} onclick={() => store.composeFrom(message, "reply_all")}>
          <Icon name="replyAll" />全員に返信
        </button>
        <button class="pill-btn" disabled={store.composing} onclick={() => store.composeFrom(message, "forward")}>
          <Icon name="forward" />転送
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .view {
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
    padding: 0 8px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--divider);
  }
  .spacer {
    flex: 1;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 16px 24px 24px;
  }
  .subject-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 20px 0 8px 48px;
  }
  h2 {
    margin: 0;
    font-size: 22px;
    font-weight: 400;
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .chip {
    padding: 0 6px;
    border-radius: 4px;
    background: var(--chip);
    color: var(--text-2);
    font-size: 12px;
    line-height: 20px;
  }
  .sender {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 12px 0 16px;
  }
  .avatar {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-size: 18px;
    margin-right: 8px;
    flex-shrink: 0;
  }
  .who {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .name {
    font-weight: 700;
    font-size: 14px;
  }
  .addr,
  .to {
    font-size: 12px;
    color: var(--text-2);
  }
  .addr {
    margin-left: 4px;
  }
  .date {
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
    margin: 0 4px;
  }
  .star.on {
    color: var(--star);
  }
  .images-banner {
    margin: 0 0 12px 48px;
    padding: 10px 14px;
    border-radius: 8px;
    background: var(--banner);
    font-size: 13px;
    color: var(--text-2);
  }
  .link {
    color: var(--accent);
    text-decoration: underline;
  }
  .loading {
    margin-left: 48px;
    color: var(--text-3);
  }
  iframe {
    display: block;
    width: calc(100% - 48px);
    margin-left: 48px;
    border: none;
    background: #fff;
    border-radius: 4px;
  }
  .attachments {
    margin: 16px 0 0 48px;
    padding-top: 12px;
    border-top: 1px solid var(--divider);
  }
  .att-title {
    font-size: 13px;
    color: var(--text-2);
    margin-bottom: 8px;
  }
  .att-list {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .att {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 260px;
    padding: 6px 10px;
    border: 1px solid var(--divider);
    border-radius: 8px;
    color: var(--text-2);
  }
  .att:hover {
    background: var(--hover);
  }
  .att-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
  }
  .att-size {
    font-size: 12px;
    flex-shrink: 0;
  }
  .actions {
    display: flex;
    gap: 12px;
    margin: 24px 0 0 48px;
  }
</style>
