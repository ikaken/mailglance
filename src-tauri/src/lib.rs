mod auth;
mod body;
mod cache;
mod config;
mod error;
mod gmail;
mod links;
mod quota;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::{StreamExt, TryStreamExt, stream};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use cache::{Cache, LabelRow, Summary};
use config::{AccountConfig, Config};
use error::{Error, Result};
use gmail::Gmail;
use quota::Quota;

/// Messages per page, as in Gmail. Each costs 20 quota units the first time it is fetched.
const PAGE_SIZE: u32 = 50;
/// Parallel API calls. Gmail's per-user quota is per minute, so bursts matter.
const CONCURRENCY: usize = 4;

struct Account {
    email: String,
    db: Mutex<Cache>,
    token: tokio::sync::Mutex<Option<(String, Instant)>>,
    quota: Quota,
}

struct AppState {
    dir: PathBuf,
    config: Mutex<Config>,
    http: reqwest::Client,
    accounts: Mutex<HashMap<String, Arc<Account>>>,
}

impl AppState {
    fn config_path(&self) -> PathBuf {
        self.dir.join("config.json")
    }

    fn db_path(&self, email: &str) -> PathBuf {
        self.dir.join("cache").join(format!("{}.sqlite", links::safe_name(email)))
    }

    fn client(&self) -> Result<(String, String)> {
        let c = self.config.lock().unwrap();
        if c.client_id.is_empty() {
            return Err(Error::new("クライアント ID が設定されていません。"));
        }
        Ok((c.client_id.clone(), c.client_secret.clone()))
    }

    fn account(&self, email: &str) -> Result<Arc<Account>> {
        if !self.config.lock().unwrap().accounts.iter().any(|a| a.email == email) {
            return Err(Error::new(format!("アカウント {email} は登録されていません。")));
        }
        let mut accounts = self.accounts.lock().unwrap();
        if let Some(a) = accounts.get(email) {
            return Ok(a.clone());
        }
        let account = Arc::new(Account {
            email: email.to_string(),
            db: Mutex::new(Cache::open(&self.db_path(email))?),
            token: tokio::sync::Mutex::new(None),
            quota: Quota::default(),
        });
        accounts.insert(email.to_string(), account.clone());
        Ok(account)
    }

    /// API client with a valid access token, refreshing it when needed.
    async fn gmail<'a>(&'a self, account: &'a Account) -> Result<Gmail<'a>> {
        let mut token = account.token.lock().await;
        if let Some((t, expires)) = token.as_ref()
            && *expires > Instant::now() + Duration::from_secs(60)
        {
            return Ok(Gmail { http: &self.http, token: t.clone(), quota: &account.quota });
        }
        let (id, secret) = self.client()?;
        let refresh = auth::load_refresh_token(&account.email)?;
        let t = auth::refresh(&self.http, &id, &secret, &refresh).await?;
        *token = Some((t.access_token.clone(), Instant::now() + Duration::from_secs(t.expires_in)));
        Ok(Gmail { http: &self.http, token: t.access_token, quota: &account.quota })
    }
}

#[derive(Serialize)]
struct AppInfo {
    configured: bool,
    client_id: String,
    accounts: Vec<AccountConfig>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum View {
    Label { id: String },
    Search { q: String },
}

impl View {
    fn key(&self) -> String {
        match self {
            View::Label { id } => format!("label:{id}"),
            View::Search { q } => format!("search:{q}"),
        }
    }
}

#[derive(Serialize)]
struct MessageBody {
    html: String,
    attachments: Vec<AttachmentInfo>,
}

#[derive(Serialize, Deserialize)]
struct AttachmentInfo {
    filename: String,
    size: i64,
}

#[derive(Serialize, Deserialize)]
struct CachedBody {
    html: String,
    attachments: Vec<AttachmentInfo>,
}

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    let c = state.config.lock().unwrap();
    AppInfo {
        configured: !c.client_id.is_empty() && !c.client_secret.is_empty(),
        client_id: c.client_id.clone(),
        accounts: c.accounts.clone(),
    }
}

#[tauri::command]
fn save_client(state: State<'_, AppState>, client_id: String, client_secret: String) -> Result<()> {
    let mut c = state.config.lock().unwrap();
    c.client_id = client_id.trim().to_string();
    // An empty secret keeps the saved one, so the settings form needn't show it.
    if !client_secret.trim().is_empty() {
        c.client_secret = client_secret.trim().to_string();
    }
    c.save(&state.config_path())
}

#[tauri::command]
async fn add_account(app: AppHandle, state: State<'_, AppState>) -> Result<String> {
    let (id, secret) = state.client()?;
    let tokens = auth::authorize(&state.http, &id, &secret, |url| {
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| Error::new(e.to_string()))
    })
    .await?;
    let refresh = tokens
        .refresh_token
        .ok_or_else(|| Error::new("リフレッシュトークンを受け取れませんでした。"))?;
    // The account isn't known yet; one profile call doesn't need shared pacing.
    let quota = Quota::default();
    let gmail = Gmail { http: &state.http, token: tokens.access_token.clone(), quota: &quota };
    let email = gmail.profile().await?.email_address;
    auth::save_refresh_token(&email, &refresh)?;

    {
        let mut c = state.config.lock().unwrap();
        if c.account_mut(&email).is_none() {
            c.accounts.push(AccountConfig { email: email.clone(), favorites: vec![] });
        }
        c.save(&state.config_path())?;
    }
    let account = state.account(&email)?;
    *account.token.lock().await = Some((
        tokens.access_token,
        Instant::now() + Duration::from_secs(tokens.expires_in),
    ));
    Ok(email)
}

#[tauri::command]
fn remove_account(app: AppHandle, state: State<'_, AppState>, email: String) -> Result<()> {
    {
        let mut c = state.config.lock().unwrap();
        c.accounts.retain(|a| a.email != email);
        c.save(&state.config_path())?;
    }
    state.accounts.lock().unwrap().remove(&email);
    auth::delete_refresh_token(&email);
    if let Some(w) = app.get_webview_window(&format!("gmail-{}", links::safe_name(&email))) {
        let _ = w.close();
    }
    let db = state.db_path(&email);
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", db.display()));
    }
    let _ = std::fs::remove_dir_all(links::gmail_webview_dir(&state.dir, &email));
    Ok(())
}

#[tauri::command]
fn set_favorite(state: State<'_, AppState>, email: String, label_id: String, on: bool) -> Result<()> {
    let mut c = state.config.lock().unwrap();
    let account = c
        .account_mut(&email)
        .ok_or_else(|| Error::new("アカウントが見つかりません。"))?;
    account.favorites.retain(|f| *f != label_id);
    if on {
        account.favorites.push(label_id);
    }
    c.save(&state.config_path())
}

#[tauri::command]
fn cached_labels(state: State<'_, AppState>, email: String) -> Result<Vec<LabelRow>> {
    state.account(&email)?.db.lock().unwrap().labels()
}

/// Applies changes made elsewhere (phone, web) since the last sync, then reloads label counts.
/// Counts cost one call per label, so they are reloaded only when the mailbox changed.
#[tauri::command]
async fn refresh_labels(state: State<'_, AppState>, email: String) -> Result<Vec<LabelRow>> {
    let account = state.account(&email)?;
    let gmail = state.gmail(&account).await?;
    if sync_history(&account, &gmail).await? {
        // Kept until the counts are saved, so a failed reload is retried next time.
        account.db.lock().unwrap().set_meta("labels_dirty", "1")?;
    }
    {
        let db = account.db.lock().unwrap();
        let cached = db.labels()?;
        if db.meta("labels_dirty")?.is_none() && !cached.is_empty() {
            return Ok(cached);
        }
    }

    let labels = gmail.labels().await?;
    let mut requests = Vec::new();
    for l in &labels {
        requests.push(gmail.label(&l.id));
    }
    let full: Vec<gmail::Label> = stream::iter(requests)
        .buffer_unordered(CONCURRENCY)
        .try_collect()
        .await?;
    let rows: Vec<LabelRow> = full
        .into_iter()
        .map(|l| LabelRow {
            id: l.id,
            name: l.name,
            kind: l.kind.unwrap_or_default(),
            unread: l.messages_unread.unwrap_or(0),
            total: l.messages_total.unwrap_or(0),
            color: l.color.and_then(|c| c.background_color),
        })
        .collect();

    let mut db = account.db.lock().unwrap();
    db.replace_labels(&rows)?;
    db.delete_meta("labels_dirty")?;
    db.labels()
}

/// Returns whether anything changed since the last sync (or the state was reset).
async fn sync_history(account: &Account, gmail: &Gmail<'_>) -> Result<bool> {
    let start = account.db.lock().unwrap().meta("history_id")?;
    let Some(start) = start else {
        let profile = gmail.profile().await?;
        account.db.lock().unwrap().set_meta("history_id", &profile.history_id)?;
        return Ok(true);
    };

    let mut changed = false;
    let mut page_token: Option<String> = None;
    loop {
        let page = match gmail.history(&start, page_token.as_deref()).await {
            Ok(p) => p,
            Err(e) if e.status == Some(404) => {
                // History is too old to replay: start over from the server's state.
                let profile = gmail.profile().await?;
                let db = account.db.lock().unwrap();
                db.clear_messages()?;
                db.set_meta("history_id", &profile.history_id)?;
                return Ok(true);
            }
            Err(e) => return Err(e),
        };
        let db = account.db.lock().unwrap();
        let records = page.history.unwrap_or_default();
        changed |= !records.is_empty();
        for record in records {
            for m in record.messages_deleted.unwrap_or_default() {
                db.delete(&m.message.id)?;
            }
            let changed = record
                .labels_added
                .into_iter()
                .chain(record.labels_removed)
                .chain(record.messages_added)
                .flatten();
            for m in changed {
                if let Some(ids) = &m.message.label_ids {
                    db.set_label_ids(&m.message.id, ids)?;
                }
            }
        }
        db.set_meta("history_id", &page.history_id)?;
        match page.next_page_token {
            Some(t) => page_token = Some(t),
            None => return Ok(changed),
        }
    }
}

#[tauri::command]
fn cached_messages(state: State<'_, AppState>, email: String, view: View) -> Result<Vec<Summary>> {
    state.account(&email)?.db.lock().unwrap().list(&view.key())
}

/// Messages already on screen, re-read from the cache after a sync. Ids no longer cached are skipped.
#[tauri::command]
fn cached_summaries(state: State<'_, AppState>, email: String, ids: Vec<String>) -> Result<Vec<Summary>> {
    let account = state.account(&email)?;
    let db = account.db.lock().unwrap();
    let mut out = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(s) = db.summary(id)? {
            out.push(s);
        }
    }
    Ok(out)
}

#[derive(Serialize)]
struct MessagePage {
    messages: Vec<Summary>,
    next_page_token: Option<String>,
    /// Gmail's estimate of the total number of results.
    estimate: i64,
}

/// One page of a view from the server. Only metadata of messages not yet cached is fetched.
/// The first page is also saved as the view's cached list.
#[tauri::command]
async fn refresh_messages(
    state: State<'_, AppState>,
    email: String,
    view: View,
    page_token: Option<String>,
) -> Result<MessagePage> {
    let account = state.account(&email)?;
    let gmail = state.gmail(&account).await?;
    let token = page_token.as_deref();
    let page = match &view {
        View::Label { id } => gmail.list(Some(id), None, token, PAGE_SIZE).await?,
        View::Search { q } => gmail.list(None, Some(q), token, PAGE_SIZE).await?,
    };
    let ids = page.ids;

    let known = account.db.lock().unwrap().known_ids(&ids)?;
    // Futures are built eagerly: closures borrowing `gmail` trip up the Send check of commands.
    let mut requests = Vec::new();
    for id in ids.iter().filter(|id| !known.contains(*id)) {
        requests.push(gmail.metadata(id));
    }
    let fetched: Vec<gmail::Message> = stream::iter(requests)
        .buffer_unordered(CONCURRENCY)
        .try_collect()
        .await?;

    let db = account.db.lock().unwrap();
    for m in &fetched {
        db.upsert(&Summary::from_message(m))?;
    }
    if page_token.is_none() {
        db.set_list(&view.key(), &ids)?;
    }
    let mut messages = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(s) = db.summary(id)? {
            messages.push(s);
        }
    }
    Ok(MessagePage { messages, next_page_token: page.next_page_token, estimate: page.estimate })
}

/// Body from the cache, or fetched and cached.
async fn load_body(state: &AppState, account: &Account, id: &str) -> Result<CachedBody> {
    if let Some(json) = account.db.lock().unwrap().body(id)?
        && let Ok(body) = serde_json::from_str::<CachedBody>(&json)
    {
        return Ok(body);
    }
    let gmail = state.gmail(account).await?;
    let msg = gmail.full(id).await?;
    let rendered = body::render(&gmail, &msg).await?;
    let body = CachedBody {
        html: rendered.html,
        attachments: rendered
            .attachments
            .into_iter()
            .map(|a| AttachmentInfo { filename: a.filename, size: a.size })
            .collect(),
    };
    let db = account.db.lock().unwrap();
    if db.summary(id)?.is_none() {
        db.upsert(&Summary::from_message(&msg))?;
    }
    db.set_body(id, &serde_json::to_string(&body)?)?;
    Ok(body)
}

#[tauri::command]
async fn get_body(state: State<'_, AppState>, email: String, id: String) -> Result<MessageBody> {
    let account = state.account(&email)?;
    let body = load_body(&state, &account, &id).await?;
    Ok(MessageBody { html: body.html, attachments: body.attachments })
}

/// Adds/removes labels (read/unread, star, archive) in Gmail and in the cache.
/// Returns the message's labels afterwards.
#[tauri::command]
async fn modify_message(
    state: State<'_, AppState>,
    email: String,
    id: String,
    add: Vec<String>,
    remove: Vec<String>,
) -> Result<Vec<String>> {
    let account = state.account(&email)?;
    let gmail = state.gmail(&account).await?;
    let add: Vec<&str> = add.iter().map(String::as_str).collect();
    let remove: Vec<&str> = remove.iter().map(String::as_str).collect();
    let msg = gmail.modify(&id, &add, &remove).await?;
    let label_ids = msg.label_ids.unwrap_or_default();
    account.db.lock().unwrap().set_label_ids(&id, &label_ids)?;
    Ok(label_ids)
}

/// Moves a message to the trash (Gmail deletes it after 30 days).
#[tauri::command]
async fn trash_message(state: State<'_, AppState>, email: String, id: String) -> Result<Vec<String>> {
    let account = state.account(&email)?;
    let gmail = state.gmail(&account).await?;
    let label_ids = gmail.trash(&id).await?.label_ids.unwrap_or_default();
    account.db.lock().unwrap().set_label_ids(&id, &label_ids)?;
    Ok(label_ids)
}

#[tauri::command]
fn open_link(app: AppHandle, state: State<'_, AppState>, email: String, url: String) -> Result<()> {
    links::open_link(&app, &state.dir, &email, &url)
}

#[tauri::command]
fn open_gmail(app: AppHandle, state: State<'_, AppState>, email: String, url: String) -> Result<()> {
    links::open_gmail(&app, &state.dir, &email, &url)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let config = Config::load(&dir.join("config.json"));
            app.manage(AppState {
                dir,
                config: Mutex::new(config),
                http: reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?,
                accounts: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            save_client,
            add_account,
            remove_account,
            set_favorite,
            cached_labels,
            refresh_labels,
            cached_messages,
            cached_summaries,
            refresh_messages,
            get_body,
            modify_message,
            trash_message,
            open_link,
            open_gmail,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
