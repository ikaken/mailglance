# Google Cloud クライアント ID 取得手順

mailglance で使う OAuth クライアント ID とシークレットを、5 つのステップで取得します。所要時間は 15〜30 分ほどです。Google Cloud の画面は変わることがあるので、名前が違う場合は近い項目を探してください。

## 1. プロジェクトを作る

1. https://console.cloud.google.com/projectcreate を開き、Gmail のアカウントでログインする。
2. プロジェクト名に「mailglance」など好きな名前を入れて「作成」を押す。
3. 画面上部のプロジェクト選択で、作ったプロジェクトが選ばれていることを確認する。

## 2. Gmail API を有効にする

1. https://console.cloud.google.com/apis/library/gmail.googleapis.com を開く。
2. 「有効にする」を押す。

## 3. OAuth 同意画面を設定する

1. https://console.cloud.google.com/auth/branding を開く（「Google Auth Platform」の「ブランディング」）。
2. 「開始」を押し、アプリ名（例：mailglance）とサポート用メールアドレスを入れる。アプリ名に「Gmail」「Google」は含めない（Google のブランド規定で認められないことがある）。
3. 対象は「外部」を選ぶ。
4. 連絡先のメールアドレスを入れ、ポリシーに同意して「作成」を押す。
5. https://console.cloud.google.com/auth/audience を開き、「テストユーザー」に使う Gmail のアドレスを全部追加する。

## 4. クライアント ID を作る

1. https://console.cloud.google.com/auth/clients を開き、「クライアントを作成」を押す。
2. アプリケーションの種類で「デスクトップ アプリ」を選ぶ。
3. 名前（例：mailglance Desktop）を入れて「作成」を押す。
4. 表示された「クライアント ID」と「クライアント シークレット」を控える。JSON をダウンロードしておくと安心。

## 5. アプリに設定する

1. アプリの設定画面に、クライアント ID とクライアント シークレットを入力する。
2. アカウントを追加し、ブラウザで Google にログインする。
3. 「Google はこのアプリを確認していません」と出たら、「詳細」→「（アプリ名）に移動」を押して進む。自分で作ったアプリなので問題ありません。

## 注意点

- テストモードのままだと、約 7 日でログインが切れる。切れないようにするには、https://console.cloud.google.com/auth/audience で公開ステータスを「本番環境に push」に切り替える（未確認アプリの警告は出たままになる）。
- クライアント シークレットは他人に渡さず、GitHub などに上げない。
