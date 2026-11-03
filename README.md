# mailglance

Gmail を Chrome の外で読むための Windows アプリ。メール内のリンクは、最後にアクティブだった Chrome のウィンドウで開きます。

設計は [docs/Gmail 専用 Windows アプリ 設計メモ.md](docs/Gmail%20専用%20Windows%20アプリ%20設計メモ.md) を参照。

## 使い方

1. [docs/Google Cloud クライアント ID 取得手順.md](docs/Google%20Cloud%20クライアント%20ID%20取得手順.md) に沿ってクライアント ID とシークレットを作る。
2. アプリの設定画面に入力して「保存」。
3. 「アカウントを追加」でブラウザから Google にログインする（複数アカウント可）。

- 画面は Gmail のウェブ版に合わせています（上部の検索、左の作成ボタンとラベル、1 行形式の一覧、50 件ずつのページ送り）。右上のボタンで閲覧ウィンドウ（右に本文）に切り替えられます。
- 一覧はキャッシュから即表示し、裏で最新に更新します（60 秒ごとに自動更新）。
- 一覧と本文から、スター・アーカイブ・削除（ゴミ箱へ）・未読／既読の切り替えができます。
- 本文のリンクは Chrome に渡します（Chrome がなければ既定のブラウザ）。外部画像は「下に画像を表示」を押すまで読み込みません。
- 返信・転送・作成・振り分け設定は、アカウントごとの Gmail ウィンドウ（WebView2）で開きます。
- キーボード ショートカットは Gmail と同じです（`j` / `k`、`o`、`u`、`e`、`#`、`s`、`Shift+U`、`r`、`c`、`/`、`n` / `p`）。一覧は設定画面にあります。

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

- Rust のテスト：`cd src-tauri && cargo test`
- 画面の型チェック：`npm run check`
- インストーラーの作成：`npm run tauri build`

## データの保存場所

- 設定：`%APPDATA%\com.ikaken.mailglance\config.json`（クライアント ID・シークレット、アカウント一覧）
- キャッシュ：同じフォルダの `cache\*.sqlite`
- リフレッシュトークン：Windows の資格情報マネージャー（`mailglance`）
- Gmail ウィンドウのログイン状態：同じフォルダの `webview\<アカウント>`
