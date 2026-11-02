//! OAuth for installed apps: browser sign-in, loopback redirect, PKCE.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use crate::error::{Error, Result};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// Read plus label changes (mark as read). Restricted scope.
const SCOPE: &str = "https://www.googleapis.com/auth/gmail.modify";
const KEYRING_SERVICE: &str = "mailglance";
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct TokenError {
    error: String,
    error_description: Option<String>,
}

fn random_b64(len: usize) -> String {
    let bytes: Vec<u8> = (0..len).map(|_| rand::random::<u8>()).collect();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Runs the sign-in flow. `open` shows the consent page in the user's browser.
pub async fn authorize(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    open: impl FnOnce(&str) -> Result<()>,
) -> Result<Tokens> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let redirect_uri = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let verifier = random_b64(32);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_b64(16);

    let auth_url = Url::parse_with_params(
        AUTH_URL,
        &[
            ("client_id", client_id),
            ("redirect_uri", &redirect_uri),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
            ("access_type", "offline"),
            ("prompt", "consent select_account"),
        ],
    )?;
    open(auth_url.as_str())?;

    let code = tokio::time::timeout(SIGN_IN_TIMEOUT, wait_for_code(&listener, &state))
        .await
        .map_err(|_| Error::new("ログインがタイムアウトしました。もう一度お試しください。"))??;

    token_request(
        http,
        &[
            ("code", code.as_str()),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", &redirect_uri),
            ("grant_type", "authorization_code"),
            ("code_verifier", &verifier),
        ],
    )
    .await
}

/// Accepts connections until Google redirects back with `code` (or `error`).
async fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await?;
        let request = String::from_utf8_lossy(&buf[..n]);
        let target = request.split_whitespace().nth(1).unwrap_or("/");
        let url = Url::parse(&format!("http://127.0.0.1{target}"))?;
        let param = |key: &str| {
            url.query_pairs()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.into_owned())
        };

        let (code, error) = (param("code"), param("error"));
        if code.is_none() && error.is_none() {
            // Favicon or other stray requests.
            respond(&mut stream, "404 Not Found", "").await;
            continue;
        }
        if param("state").as_deref() != Some(state) {
            respond(&mut stream, "400 Bad Request", "不正なリクエストです。").await;
            return Err(Error::new("ログインの応答を確認できませんでした（state 不一致）。"));
        }
        if let Some(error) = error {
            respond(&mut stream, "200 OK", "ログインが中止されました。このタブを閉じてください。").await;
            let hint = if error == "access_denied" {
                "\nアプリがテストモードの場合、Google Cloud の「対象」（auth/audience）でこのアドレスをテストユーザーに追加してください。"
            } else {
                ""
            };
            return Err(Error::new(format!("ログインが中止されました: {error}{hint}")));
        }
        respond(
            &mut stream,
            "200 OK",
            "mailglance へのログインが完了しました。このタブを閉じてアプリに戻ってください。",
        )
        .await;
        return Ok(code.unwrap_or_default());
    }
}

async fn respond(stream: &mut tokio::net::TcpStream, status: &str, message: &str) {
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>mailglance</title>\
         <body style=\"font-family:sans-serif;padding:40px\"><p>{message}</p></body>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

pub async fn refresh(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<Tokens> {
    token_request(
        http,
        &[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ],
    )
    .await
}

async fn token_request(http: &reqwest::Client, form: &[(&str, &str)]) -> Result<Tokens> {
    let resp = http.post(TOKEN_URL).form(form).send().await?;
    let status = resp.status();
    if status.is_success() {
        return Ok(resp.json().await?);
    }
    let text = resp.text().await.unwrap_or_default();
    let message = match serde_json::from_str::<TokenError>(&text) {
        Ok(e) if e.error == "invalid_grant" => {
            "ログインの有効期限が切れました。アカウントを追加し直してください。".to_string()
        }
        Ok(e) => format!("認証エラー: {} {}", e.error, e.error_description.unwrap_or_default()),
        Err(_) => format!("認証エラー: {status} {text}"),
    };
    Err(Error::http(status.as_u16(), message))
}

fn keyring_entry(email: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(KEYRING_SERVICE, email)?)
}

pub fn save_refresh_token(email: &str, token: &str) -> Result<()> {
    Ok(keyring_entry(email)?.set_password(token)?)
}

pub fn load_refresh_token(email: &str) -> Result<String> {
    keyring_entry(email)?
        .get_password()
        .map_err(|_| Error::new(format!("{email} のログイン情報がありません。アカウントを追加し直してください。")))
}

pub fn delete_refresh_token(email: &str) {
    if let Ok(entry) = keyring_entry(email) {
        let _ = entry.delete_credential();
    }
}
