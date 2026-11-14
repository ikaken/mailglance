# mailglance

Gmail を Chrome の外で読むための Windows アプリ。メール内のリンクは、最後にアクティブだった Chrome のウィンドウで開きます。

> mailglance は非公式のアプリです。Google や Gmail とは関係がありません。Gmail は Google LLC の商標です。使うには、自分の Google Cloud でクライアント ID とシークレットを作る必要があります。

設計は [docs/Gmail 専用 Windows アプリ 設計メモ.md](docs/Gmail%20専用%20Windows%20アプリ%20設計メモ.md) を参照。

## 使い方

1. [docs/Google Cloud クライアント ID 取得手順.md](docs/Google%20Cloud%20クライアント%20ID%20取得手順.md) に沿ってクライアント ID とシークレットを作る。
2. アプリの設定画面に入力して「保存」。
3. 「アカウントを追加」でブラウザから Google にログインする（複数アカウント可）。

- 画面は Gmail のウェブ版に合わせています（上部の検索、左のアカウントのアイコンと作成ボタン・ラベル、1 行形式の一覧）。
  - 一覧は下へスクロールすると 50 件ずつ続きを読み込みます。
  - 右上のボタンで、閲覧ウィンドウを「なし → 右 → 下」の順に切り替えます。
  - ラベルの列の幅と、一覧と閲覧ウィンドウの境界は、ドラッグで動かせます（ダブルクリックで元に戻る。幅は保存されます）。
- 一覧はキャッシュから即表示し、裏で最新に更新します（60 秒ごとに自動更新）。
- 本文のヘッダーから、アーカイブ・削除（ゴミ箱へ）・未読／既読の切り替え・スターができます。アーカイブ済みやゴミ箱のメールでは、アーカイブの位置が「受信トレイに移動」になります。一覧の行には「Gmail で開く」ボタンが出ます。
- 本文のリンクは Chrome に渡します（Chrome がなければ既定のブラウザ）。外部画像は「下に画像を表示」を押すまで読み込みません。
- 返信・全員に返信・転送は、Gmail API で下書きを作り、アカウントごとの Gmail ウィンドウ（WebView2）で開きます。
  - 返信は元のスレッドにつながり、宛先と引用が入ります。
  - 転送は、元のメールの添付ファイルとインライン画像も付きます（合計 25 MB まで。超えるとエラーになるので、Gmail の画面から転送してください）。
  - 送らずに閉じた下書きは、Gmail の下書きに残ります。
  - 新規作成と振り分け設定も、同じ Gmail ウィンドウで開きます。
- Gmail ウィンドウは、アカウントごとに初回だけ Google へのログインが必要です（アプリの OAuth とは別のログイン状態）。
- キーボード ショートカットは Gmail と同じです（`j` / `k`、`o`、`u`、`e`、`#`、`s`、`Shift+U`、`r`、`a`、`f`、`c`、`/`）。一覧は設定画面にあります。

## API の使用量

Gmail API の上限（1 ユーザーあたり 1 分 6,000 単位）を超えないよう、アプリが使用量を数えて待ちます。

- 一覧の続きの読み込みは、スクロールしたときだけです。1 回あたり最大約 1,000 単位で、キャッシュ済みのメールは読み直しません。
- 自動更新は、最新の 50 件の一覧取得（5 単位）と、新着分だけです。
- 返信・全員に返信は約 30 単位です。転送は約 70 単位と、添付 1 つにつき 20 単位です。

## 開発

必要なもの：Rust（stable, MSVC）、Node.js、Visual Studio Build Tools（C++）、WebView2。

```powershell
npm install
npm run tauri dev
```

- `cargo` が PATH にない場合は `%USERPROFILE%\.cargo\bin` をユーザーの PATH に追加し、PowerShell を開き直す。

  ```powershell
  [Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path", "User") + ";$env:USERPROFILE\.cargo\bin", "User")
  ```

- Rust のテスト：`cd src-tauri && cargo test`（宛先の計算・下書きの組み立て・文字コードなど）
- 画面の型チェック：`npm run check`
- インストーラーの作成：`npm run tauri build`（更新用の署名が必要です。下の「配布と自動更新」を参照）

## 配布と自動更新

- 配布は、NSIS のインストーラー（`mailglance_<版>_x64-setup.exe`）です。ユーザー単位でインストールされ（`%LOCALAPPDATA%\mailglance`、管理者権限は不要）、日本語の画面です。アンインストールしても、設定・キャッシュ・ログイン状態は残ります。
- アプリは、起動の少しあとと、その後 1 日に 1 回、GitHub Releases の `latest.json` を見て、新しい版があれば画面右下に案内を出します（設定画面の「更新を確認」からも確認できます）。「今すぐ更新」で、ダウンロード、署名の検証、インストール、再起動までを行います。署名が合わないファイルは、インストールされません。
- 更新の署名キーは、Tauri の updater 用のものです（Windows のコード署名ではありません）。インストーラーには Windows のコード署名がないため、初回のインストールで SmartScreen の警告が出ることがあります。
- ビルドの手順：

  ```powershell
  $env:TAURI_SIGNING_PRIVATE_KEY = "$env:USERPROFILE\.tauri\mailglance.key"
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "<キーのパスワード>"
  npm run tauri build
  ```

  `src-tauri/target/release/bundle/nsis/` に、インストーラーと署名ファイル（`.sig`）ができます。
- 署名キーは、リポジトリに入れません。公開鍵だけが `src-tauri/tauri.conf.json` にあります。**秘密鍵をなくすと、インストール済みのアプリが更新できなくなります**（入れ直しになります）。バックアップを取ってください。
- 公開（Releases への添付と `latest.json` の作成）の自動化は、まだありません。

## データの保存場所

- 設定：`%APPDATA%\com.ikaken.mailglance\config.json`（クライアント ID・シークレット、アカウント一覧）
- キャッシュ：同じフォルダの `cache\*.sqlite`
- リフレッシュトークン：Windows の資格情報マネージャー（`mailglance`）
- Gmail ウィンドウのログイン状態：同じフォルダの `webview\<アカウント>`

## セキュリティとプライバシー

- アプリが通信するのは、Google の API（Gmail、OAuth）だけです。作者のサーバーには、何も送りません。
- 取得したメールの一覧と本文は、ローカルの SQLite（`cache\*.sqlite`）に平文で保存します。ディスクの暗号化（BitLocker など）をおすすめします。
- クライアント シークレットは `config.json` に平文で保存します。リフレッシュトークンは、Windows の資格情報マネージャーに保存します。
- 脆弱性の報告は [SECURITY.md](SECURITY.md) を参照してください。

## ライセンス

[MIT License](LICENSE)
