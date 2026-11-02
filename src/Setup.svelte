<script lang="ts">
  import { api, errorText, type AppInfo } from "./api";

  let {
    info,
    onchange,
    onclose,
  }: { info: AppInfo; onchange: (added?: string) => void; onclose?: () => void } = $props();

  // The form starts from the saved value and is edited independently afterwards.
  // svelte-ignore state_referenced_locally
  let clientId = $state(info.client_id);
  let clientSecret = $state("");
  let busy = $state(false);
  let message = $state("");
  let error = $state("");

  async function save() {
    error = message = "";
    try {
      await api.saveClient(clientId, clientSecret);
      clientSecret = "";
      message = "保存しました。";
      onchange();
    } catch (e) {
      error = errorText(e);
    }
  }

  async function addAccount() {
    error = message = "";
    busy = true;
    try {
      const email = await api.addAccount();
      message = `${email} を追加しました。`;
      onchange(email);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function remove(email: string) {
    if (!confirm(`${email} をこのアプリから削除しますか？（Gmail のメールは消えません）`)) return;
    try {
      await api.removeAccount(email);
      onchange();
    } catch (e) {
      error = errorText(e);
    }
  }
</script>

<div class="setup">
  <div class="head">
    <h1>設定</h1>
    {#if onclose}<button class="btn" onclick={onclose}>閉じる</button>{/if}
  </div>

  <section>
    <h2>1. OAuth クライアント</h2>
    <p class="hint">
      Google Cloud で作成したデスクトップ アプリのクライアント ID とシークレットを入力します。
      作り方はリポジトリの docs/Google Cloud クライアント ID 取得手順.md を参照してください。
    </p>
    <label>クライアント ID <input bind:value={clientId} placeholder="xxxx.apps.googleusercontent.com" /></label>
    <label>
      クライアント シークレット
      <input
        type="password"
        bind:value={clientSecret}
        placeholder={info.configured ? "（保存済み。変更するときだけ入力）" : "GOCSPX-..."}
      />
    </label>
    <button class="btn primary" onclick={save} disabled={!clientId.trim() || (!info.configured && !clientSecret.trim())}>
      保存
    </button>
  </section>

  <section>
    <h2>2. アカウント</h2>
    <p class="hint">「アカウントを追加」を押すとブラウザで Google のログイン画面が開きます。</p>
    <ul>
      {#each info.accounts as a (a.email)}
        <li><span>{a.email}</span> <button class="btn" onclick={() => remove(a.email)}>削除</button></li>
      {:else}
        <li class="hint">まだアカウントがありません。</li>
      {/each}
    </ul>
    <button class="btn primary" onclick={addAccount} disabled={!info.configured || busy}>
      {busy ? "ブラウザでログインしてください…" : "アカウントを追加"}
    </button>
  </section>

  {#if message}<p class="ok">{message}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .setup {
    max-width: 640px;
    margin: 0 auto;
    padding: 32px 24px;
    overflow-y: auto;
    height: 100%;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h1 {
    font-size: 22px;
  }
  h2 {
    font-size: 16px;
    margin: 0 0 8px;
  }
  section {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 16px;
  }
  label {
    display: block;
    margin-bottom: 12px;
    color: var(--text-muted);
  }
  label input {
    display: block;
    width: 100%;
    margin-top: 4px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
  }
  ul {
    list-style: none;
    padding: 0;
  }
  li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .hint {
    color: var(--text-muted);
    font-size: 13px;
  }
  .ok {
    color: var(--accent);
  }
  .error {
    color: var(--error-text);
    background: var(--error-bg);
    padding: 8px 12px;
    border-radius: 6px;
    white-space: pre-wrap;
  }
</style>
