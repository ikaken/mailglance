//! Thin Gmail REST API client.

use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};
use crate::quota::{Quota, cost};

const BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const UPLOAD_BASE: &str = "https://gmail.googleapis.com/upload/gmail/v1/users/me";
/// Retries after 2 s and 8 s; beyond that the per-minute quota needs a real pause.
const MAX_RETRIES: u32 = 2;
/// Headers shown in the message list and the message header.
const LIST_HEADERS: [&str; 4] = ["From", "To", "Subject", "Date"];
/// Headers needed to address and thread a reply.
const REPLY_HEADERS: [&str; 9] =
    ["From", "To", "Cc", "Reply-To", "Subject", "Date", "Message-ID", "References", "In-Reply-To"];

pub struct Gmail<'a> {
    pub http: &'a reqwest::Client,
    pub token: String,
    pub quota: &'a Quota,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub email_address: String,
    pub history_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub messages_total: Option<i64>,
    pub messages_unread: Option<i64>,
    pub color: Option<LabelColor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelColor {
    pub background_color: Option<String>,
}

#[derive(Deserialize)]
struct LabelList {
    labels: Option<Vec<Label>>,
}

#[derive(Deserialize)]
struct IdRef {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageList {
    messages: Option<Vec<IdRef>>,
    next_page_token: Option<String>,
    result_size_estimate: Option<i64>,
}

pub struct ListPage {
    pub ids: Vec<String>,
    pub next_page_token: Option<String>,
    pub estimate: i64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    #[serde(default)]
    pub thread_id: String,
    pub label_ids: Option<Vec<String>>,
    pub snippet: Option<String>,
    pub internal_date: Option<String>,
    pub payload: Option<Part>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub mime_type: Option<String>,
    pub filename: Option<String>,
    pub headers: Option<Vec<Header>>,
    pub body: Option<Body>,
    pub parts: Option<Vec<Part>>,
}

#[derive(Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    pub size: Option<i64>,
    pub data: Option<String>,
    pub attachment_id: Option<String>,
}

#[derive(Deserialize)]
pub struct Draft {
    pub message: Message,
}

#[derive(Deserialize)]
struct Attachment {
    data: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub history: Option<Vec<HistoryRecord>>,
    pub history_id: String,
    pub next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub messages_added: Option<Vec<HistoryMessage>>,
    pub messages_deleted: Option<Vec<HistoryMessage>>,
    pub labels_added: Option<Vec<HistoryMessage>>,
    pub labels_removed: Option<Vec<HistoryMessage>>,
}

#[derive(Deserialize)]
pub struct HistoryMessage {
    pub message: Message,
}

impl Part {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .as_ref()?
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }
}

impl Gmail<'_> {
    /// Retries rate-limit errors (429, or 403 with a quota reason) with backoff.
    /// Every call first waits for room in the per-minute quota (see `quota`).
    async fn send<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder, units: u32) -> Result<T> {
        let req = req.bearer_auth(&self.token);
        let mut delay = Duration::from_secs(2);
        for attempt in 0.. {
            let attempt_req = req.try_clone().ok_or_else(|| Error::new("request not cloneable"))?;
            self.quota.acquire(units).await;
            let resp = attempt_req.send().await?;
            let status = resp.status();
            #[cfg(debug_assertions)]
            eprintln!(
                "[gmail] {:?} {} {}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                resp.url().path().trim_start_matches("/gmail/v1/users/me"),
                status.as_u16()
            );
            if status.is_success() {
                return Ok(resp.json().await?);
            }
            let text = resp.text().await.unwrap_or_default();
            let rate_limited = status.as_u16() == 429
                || (status.as_u16() == 403 && (text.contains("rateLimitExceeded") || text.contains("Quota exceeded")));
            if rate_limited && attempt < MAX_RETRIES {
                tokio::time::sleep(delay).await;
                delay *= 4;
                continue;
            }
            if rate_limited {
                return Err(Error::http(
                    status.as_u16(),
                    "Gmail の利用上限（1 分あたり）に達しました。1 分ほど待つと自動で再開します。",
                ));
            }
            return Err(Error::http(status.as_u16(), format!("Gmail API エラー {status}: {text}")));
        }
        unreachable!()
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)], units: u32) -> Result<T> {
        self.send(self.http.get(format!("{BASE}{path}")).query(query), units).await
    }

    pub async fn profile(&self) -> Result<Profile> {
        self.get("/profile", &[], cost::GET_PROFILE).await
    }

    pub async fn labels(&self) -> Result<Vec<Label>> {
        Ok(self
            .get::<LabelList>("/labels", &[], cost::LABELS_LIST)
            .await?
            .labels
            .unwrap_or_default())
    }

    /// The list endpoint omits counts; the per-label endpoint has them.
    pub async fn label(&self, id: &str) -> Result<Label> {
        self.get(&format!("/labels/{id}"), &[], cost::LABELS_GET).await
    }

    pub async fn list(
        &self,
        label: Option<&str>,
        q: Option<&str>,
        page_token: Option<&str>,
        max: u32,
    ) -> Result<ListPage> {
        let max = max.to_string();
        let mut query = vec![("maxResults", max.as_str())];
        if let Some(label) = label {
            query.push(("labelIds", label));
            if label == "SPAM" || label == "TRASH" {
                query.push(("includeSpamTrash", "true"));
            }
        }
        if let Some(q) = q.filter(|q| !q.is_empty()) {
            query.push(("q", q));
        }
        if let Some(token) = page_token {
            query.push(("pageToken", token));
        }
        let list: MessageList = self.get("/messages", &query, cost::MESSAGES_LIST).await?;
        Ok(ListPage {
            ids: list.messages.unwrap_or_default().into_iter().map(|m| m.id).collect(),
            next_page_token: list.next_page_token,
            estimate: list.result_size_estimate.unwrap_or(0),
        })
    }

    pub async fn metadata(&self, id: &str) -> Result<Message> {
        let mut query = vec![("format", "metadata")];
        query.extend(LIST_HEADERS.iter().map(|h| ("metadataHeaders", *h)));
        self.get(&format!("/messages/{id}"), &query, cost::MESSAGES_GET).await
    }

    pub async fn reply_headers(&self, id: &str) -> Result<Message> {
        let mut query = vec![("format", "metadata")];
        query.extend(REPLY_HEADERS.iter().map(|h| ("metadataHeaders", *h)));
        self.get(&format!("/messages/{id}"), &query, cost::MESSAGES_GET).await
    }

    /// Creates a draft from a base64url RFC 822 message; `thread_id` keeps a reply in its thread.
    pub async fn create_draft(&self, raw: &str, thread_id: Option<&str>) -> Result<Draft> {
        let mut message = serde_json::json!({ "raw": raw });
        if let Some(t) = thread_id {
            message["threadId"] = t.into();
        }
        let req = self.http.post(format!("{BASE}/drafts")).json(&serde_json::json!({ "message": message }));
        self.send(req, cost::DRAFTS_CREATE).await
    }

    /// Creates a draft from a raw RFC 822 message by media upload, which takes up to 35 MB
    /// (the JSON form is for small messages). The draft is not part of any thread.
    pub async fn create_draft_upload(&self, rfc822: Vec<u8>) -> Result<Draft> {
        let req = self
            .http
            .post(format!("{UPLOAD_BASE}/drafts?uploadType=media"))
            .header("Content-Type", "message/rfc822")
            .body(rfc822);
        self.send(req, cost::DRAFTS_CREATE).await
    }

    pub async fn full(&self, id: &str) -> Result<Message> {
        self.get(&format!("/messages/{id}"), &[("format", "full")], cost::MESSAGES_GET_FULL)
            .await
    }

    pub async fn attachment(&self, message_id: &str, attachment_id: &str) -> Result<String> {
        let path = format!("/messages/{message_id}/attachments/{attachment_id}");
        let a: Attachment = self.get(&path, &[], cost::ATTACHMENTS_GET).await?;
        Ok(a.data)
    }

    pub async fn history(&self, start: &str, page: Option<&str>) -> Result<HistoryPage> {
        let mut query = vec![("startHistoryId", start)];
        if let Some(page) = page {
            query.push(("pageToken", page));
        }
        self.get("/history", &query, cost::HISTORY_LIST).await
    }

    pub async fn modify(&self, id: &str, add: &[&str], remove: &[&str]) -> Result<Message> {
        let body = serde_json::json!({ "addLabelIds": add, "removeLabelIds": remove });
        let req = self.http.post(format!("{BASE}/messages/{id}/modify")).json(&body);
        self.send(req, cost::MESSAGES_MODIFY).await
    }

    pub async fn trash(&self, id: &str) -> Result<Message> {
        // A POST without a body carries no Content-Length, and Google's front end answers
        // 411 Length Required; so the header is set explicitly.
        let req = self
            .http
            .post(format!("{BASE}/messages/{id}/trash"))
            .header(reqwest::header::CONTENT_LENGTH, 0);
        self.send(req, cost::MESSAGES_TRASH).await
    }
}
