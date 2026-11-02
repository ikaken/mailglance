//! Opening links outside the app, and the embedded Gmail window.

use std::path::{Path, PathBuf};

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::error::{Error, Result};

const GMAIL_URL: &str = "https://mail.google.com/mail/u/0/";

/// Opens a link from a message.
///
/// http(s) goes to Chrome without a profile flag, so Chrome uses the window that was
/// active last — the one the user was working in. Without Chrome, the default browser.
/// mailto opens Gmail's compose screen for `account`.
pub fn open_link(app: &AppHandle, data_dir: &Path, account: &str, raw: &str) -> Result<()> {
    let url = Url::parse(raw)?;
    match url.scheme() {
        "http" | "https" => open_external(app, &url),
        "mailto" => {
            let compose = Url::parse_with_params(
                "https://mail.google.com/mail/u/0/",
                &[("extsrc", "mailto"), ("url", url.as_str())],
            )?;
            open_gmail(app, data_dir, account, compose.as_str())
        }
        other => Err(Error::new(format!("このリンクは開けません（{other}:）"))),
    }
}

#[cfg(windows)]
fn chrome_path() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    const APP_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\chrome.exe";
    let from_registry = [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE].into_iter().find_map(|root| {
        let path: String = RegKey::predef(root).open_subkey(APP_PATH).ok()?.get_value("").ok()?;
        Some(PathBuf::from(path))
    });
    from_registry.into_iter().chain(
        ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .iter()
            .filter_map(|v| std::env::var_os(v))
            .map(|base| PathBuf::from(base).join(r"Google\Chrome\Application\chrome.exe")),
    )
    .find(|p| p.is_file())
}

#[cfg(not(windows))]
fn chrome_path() -> Option<PathBuf> {
    None
}

/// Folder-safe and window-label-safe form of an email address.
pub fn safe_name(email: &str) -> String {
    email
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

pub fn gmail_webview_dir(data_dir: &Path, email: &str) -> PathBuf {
    data_dir.join("webview").join(safe_name(email))
}

/// Hosts the Gmail window may navigate to itself (Gmail and Google sign-in).
fn is_google_host(url: &Url) -> bool {
    if url.scheme() == "about" || url.scheme() == "data" || url.scheme() == "blob" {
        return true;
    }
    let host = url.host_str().unwrap_or_default();
    ["google.com", "google.co.jp", "gstatic.com", "googleusercontent.com", "youtube.com"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Opens Gmail's own web UI (reply, compose, filter settings) in a window per account.
/// Each account gets its own WebView2 profile so sign-ins don't collide.
pub fn open_gmail(app: &AppHandle, data_dir: &Path, email: &str, url: &str) -> Result<()> {
    let url = Url::parse(if url.is_empty() { GMAIL_URL } else { url })?;
    let label = format!("gmail-{}", safe_name(email));
    if let Some(window) = app.get_webview_window(&label) {
        window.navigate(url)?;
        let _ = window.unminimize();
        window.set_focus()?;
        return Ok(());
    }

    let nav_app = app.clone();
    let popup_app = app.clone();
    WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title(format!("Gmail - {email}"))
        .inner_size(1100.0, 800.0)
        .data_directory(gmail_webview_dir(data_dir, email))
        .on_navigation(move |u| {
            if is_google_host(u) {
                return true;
            }
            let _ = open_external(&nav_app, u);
            false
        })
        .on_new_window(move |u, _features| {
            // Links clicked inside Gmail open as new windows; send them to the browser.
            let _ = open_external(&popup_app, &u);
            NewWindowResponse::Deny
        })
        .build()?;
    Ok(())
}

fn open_external(app: &AppHandle, url: &Url) -> Result<()> {
    if let Some(chrome) = chrome_path().filter(|_| matches!(url.scheme(), "http" | "https")) {
        std::process::Command::new(chrome).arg(url.as_str()).spawn()?;
        return Ok(());
    }
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|e| Error::new(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_hosts() {
        let ok = |s: &str| is_google_host(&Url::parse(s).unwrap());
        assert!(ok("https://mail.google.com/mail/u/0/"));
        assert!(ok("https://accounts.google.com/signin"));
        assert!(!ok("https://evilgoogle.com/"));
        assert!(!ok("https://example.com/"));
    }
}
