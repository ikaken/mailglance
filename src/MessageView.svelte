<script lang="ts">
  import { api, errorText, gmailUrls, type MessageBody, type Summary } from "./api";

  let { email, message }: { email: string; message: Summary } = $props();

  let body = $state<MessageBody | null>(null);
  let error = $state("");
  let showImages = $state(false);
  let frame = $state<HTMLIFrameElement>();

  $effect(() => {
    const id = message.id;
    body = null;
    error = "";
    showImages = false;
    api
      .getBody(email, id)
      .then((b) => {
        if (message.id === id) body = b;
      })
      .catch((e) => {
        if (message.id === id) error = errorText(e);
      });
  });

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
<style>body{margin:16px;font-family:"Segoe UI","Yu Gothic UI",Meiryo,sans-serif;font-size:14px;color:#111;background:#fff;overflow-wrap:anywhere}img{max-width:100%;height:auto}a{color:#1a73e8}</style>
</head><body>${body.html}</body></html>`;
  });

  // Every link goes to the app, which hands it to the last active Chrome window.
  function hookLinks() {
    const doc = frame?.contentDocument;
    if (!doc) return;
    const handler = (ev: MouseEvent) => {
      const a = (ev.target as Element | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
      if (!a) return;
      ev.preventDefault();
      const href = a.getAttribute("href") ?? "";
      if (/^(https?:|mailto:)/i.test(href.trim())) {
        api.openLink(email, href.trim()).catch((e) => (error = errorText(e)));
      }
    };
    doc.addEventListener("click", handler);
    doc.addEventListener("auxclick", handler);
    // Forms are already blocked by the sandbox; this keeps them from showing an error page.
    doc.addEventListener("submit", (ev) => ev.preventDefault());
  }

  function formatSize(n: number) {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }
</script>

<div class="view">
  <header>
    <h2>{message.subject || "（件名なし）"}</h2>
    <div class="meta">
      <div>
        <div><span class="k">差出人</span>{message.from}</div>
        {#if message.to}<div><span class="k">宛先</span>{message.to}</div>{/if}
        <div><span class="k">日時</span>{new Date(message.date).toLocaleString("ja-JP")}</div>
      </div>
      <div class="actions">
        <button class="btn" onclick={() => api.openGmail(email, gmailUrls.thread(message.thread_id))}>
          返信・転送（Gmail）
        </button>
        {#if hasRemoteImages && !showImages}
          <button class="btn" onclick={() => (showImages = true)}>画像を表示</button>
        {/if}
      </div>
    </div>
    {#if body && body.attachments.length}
      <div class="attachments">
        添付：
        {#each body.attachments as a}
          <span class="att">{a.filename}（{formatSize(a.size)}）</span>
        {/each}
        <span class="hint">（保存は Gmail の画面から）</span>
      </div>
    {/if}
  </header>

  {#if error}
    <p class="error">{error}</p>
  {:else if !body}
    <p class="loading">読み込み中…</p>
  {:else}
    <iframe bind:this={frame} title="本文" sandbox="allow-same-origin" {srcdoc} onload={hookLinks}></iframe>
  {/if}
</div>

<style>
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
  }
  header {
    padding: 14px 18px 10px;
    border-bottom: 1px solid var(--border);
  }
  h2 {
    margin: 0 0 8px;
    font-size: 18px;
    overflow-wrap: anywhere;
  }
  .meta {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 13px;
    color: var(--text-muted);
  }
  .meta > div:first-child {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .k {
    display: inline-block;
    width: 4em;
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex-shrink: 0;
  }
  .attachments {
    margin-top: 8px;
    font-size: 13px;
    color: var(--text-muted);
  }
  .att {
    margin-right: 10px;
    color: var(--text);
  }
  .hint {
    font-size: 12px;
  }
  iframe {
    flex: 1;
    border: none;
    width: 100%;
    background: #fff;
  }
  .loading {
    padding: 18px;
    color: var(--text-muted);
  }
  .error {
    margin: 18px;
    color: var(--error-text);
    background: var(--error-bg);
    padding: 8px 12px;
    border-radius: 6px;
    white-space: pre-wrap;
  }
</style>
