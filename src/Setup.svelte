<script lang="ts">
  import { api, avatarColor, errorText, gmailUrls } from "./api";
  import Icon from "./Icon.svelte";
  import { store } from "./store.svelte";

  const info = $derived(store.info!);
  // The form starts from the saved value and is edited independently afterwards.
  let clientId = $state(store.info?.client_id ?? "");
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
      await store.loadInfo();
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
      await store.loadInfo(email);
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
      await store.loadInfo();
    } catch (e) {
      error = errorText(e);
    }
  }

  const shortcuts: [string, string][] = [
    ["j / k", "次（古い）/ 前（新しい）のメール"],
    ["o / Enter", "メールを開く"],
    ["u", "一覧に戻る"],
    ["e", "アーカイブ"],
    ["#", "削除"],
    ["s", "スター"],
    ["Shift + U", "未読 / 既読にする"],
    ["r", "返信（Gmail の返信画面を開く）"],
    ["a", "全員に返信"],
    ["f", "転送"],
    ["c", "作成（Gmail で開く）"],
    ["/", "検索"],
  ];
</script>

<div class="setup">
  <div class="head">
    <h1>設定</h1>
    {#if !store.needsSetup}
      <button class="icon-btn" title="閉じる" onclick={() => (store.showSettings = false)}>
        <Icon name="close" />
      </button>
    {/if}
  </div>

  <section>
    <h2>OAuth クライアント</h2>
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
    <button
      class="pill-btn filled"
      onclick={save}
      disabled={!clientId.trim() || (!info.configured && !clientSecret.trim())}
    >
      保存
    </button>
  </section>

  <section>
    <h2>アカウント</h2>
    <p class="hint">「アカウントを追加」を押すとブラウザで Google のログイン画面が開きます。</p>
    <ul>
      {#each info.accounts as a (a.email)}
        <li>
          <span class="avatar" style:background={avatarColor(a.email)}>{a.email[0].toUpperCase()}</span>
          <span class="email">{a.email}</span>
          <button class="pill-btn" onclick={() => api.openGmail(a.email)}>Gmail を開く</button>
          <button class="pill-btn" onclick={() => api.openGmail(a.email, gmailUrls.filters())}>
            <Icon name="tune" size={18} />振り分け設定
          </button>
          <button class="icon-btn" title="削除" onclick={() => remove(a.email)}><Icon name="delete" /></button>
        </li>
      {:else}
        <li class="hint">まだアカウントがありません。</li>
      {/each}
    </ul>
    <button class="pill-btn filled" onclick={addAccount} disabled={!info.configured || busy}>
      <Icon name="add" size={18} />{busy ? "ブラウザでログインしてください…" : "アカウントを追加"}
    </button>
  </section>

  {#if message}<p class="ok">{message}</p>{/if}
  {#if error}<div class="error-banner">{error}</div>{/if}

  {#if !store.needsSetup}
    <section>
      <h2>キーボード ショートカット（Gmail と同じ）</h2>
      <table>
        <tbody>
          {#each shortcuts as [key, what]}
            <tr><td><kbd>{key}</kbd></td><td>{what}</td></tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</div>

<style>
  .setup {
    max-width: 760px;
    margin: 0 auto;
    padding: 24px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
  }
  h1 {
    font-size: 22px;
    font-weight: 400;
    margin: 0;
  }
  h2 {
    font-size: 16px;
    font-weight: 500;
    margin: 0 0 8px;
  }
  section {
    border: 1px solid var(--divider);
    border-radius: 12px;
    padding: 16px 20px;
    margin-bottom: 16px;
    background: var(--card);
  }
  label {
    display: block;
    margin-bottom: 12px;
    color: var(--text-2);
    font-size: 13px;
  }
  label input {
    display: block;
    width: 100%;
    margin-top: 4px;
    padding: 8px 12px;
    border: 1px solid var(--outline);
    border-radius: 8px;
    background: var(--card);
    font-size: 14px;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0 0 12px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 0;
    border-bottom: 1px solid var(--divider);
  }
  .avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    flex-shrink: 0;
  }
  .email {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .hint {
    color: var(--text-2);
    font-size: 13px;
  }
  .ok {
    color: var(--accent);
  }
  table {
    border-collapse: collapse;
    font-size: 13px;
  }
  td {
    padding: 4px 16px 4px 0;
    color: var(--text-2);
  }
  kbd {
    display: inline-block;
    min-width: 24px;
    padding: 1px 6px;
    border: 1px solid var(--divider);
    border-radius: 4px;
    background: var(--page);
    font-family: inherit;
    color: var(--text);
  }
</style>
