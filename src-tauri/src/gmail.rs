//! Thin Gmail REST API client.

use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

const BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
/// Retries after 2 s and 8 s; beyond that the per-minute quota needs a real pause.
const MAX_RETRIES: u32 = 2;
/// Headers shown in the message list and the message header.
const LIST_HEADERS: [&str; 4] = ["From", "To", "Subject", "Date"];

pub struct Gmail<'a> {
    pub http: &'a reqwest::Client,
    pub token: String,
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
struct MessageList {
    messages: Option<Vec<IdRef>>,
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
    async fn send<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T> {
        let req = req.bearer_auth(&self.token);
        let mut delay = Duration::from_secs(2);
        for attempt in 0.. {
            let attempt_req = req.try_clone().ok_or_else(|| Error::new("request not cloneable"))?;
            let resp = attempt_req.send().await?;
            let status = resp.status();
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

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        self.send(self.http.get(format!("{BASE}{path}")).query(query)).await
    }

    pub async fn profile(&self) -> Result<Profile> {
        self.get("/profile", &[]).await
    }

    pub async fn labels(&self) -> Result<Vec<Label>> {
        Ok(self.get::<LabelList>("/labels", &[]).await?.labels.unwrap_or_default())
    }

    /// The list endpoint omits counts; the per-label endpoint has them.
    pub async fn label(&self, id: &str) -> Result<Label> {
        self.get(&format!("/labels/{id}"), &[]).await
    }

    pub async fn list_ids(&self, label: Option<&str>, q: Option<&str>, max: u32) -> Result<Vec<String>> {
        let max = max.to_string();
        let mut query = vec![("maxResults", max.as_str())];
        if let Some(label) = label {
            query.push(("labelIds", label));
        }
        if let Some(q) = q {
            query.push(("q", q));
        }
        let list: MessageList = self.get("/messages", &query).await?;
        Ok(list.messages.unwrap_or_default().into_iter().map(|m| m.id).collect())
    }

    pub async fn metadata(&self, id: &str) -> Result<Message> {
        let mut query = vec![("format", "metadata")];
        query.extend(LIST_HEADERS.iter().map(|h| ("metadataHeaders", *h)));
        self.get(&format!("/messages/{id}"), &query).await
    }

    pub async fn full(&self, id: &str) -> Result<Message> {
        self.get(&format!("/messages/{id}"), &[("format", "full")]).await
    }

    pub async fn attachment(&self, message_id: &str, attachment_id: &str) -> Result<String> {
        let a: Attachment = self
            .get(&format!("/messages/{message_id}/attachments/{attachment_id}"), &[])
            .await?;
        Ok(a.data)
    }

    pub async fn history(&self, start: &str, page: Option<&str>) -> Result<HistoryPage> {
        let mut query = vec![("startHistoryId", start)];
        if let Some(page) = page {
            query.push(("pageToken", page));
        }
        self.get("/history", &query).await
    }

    pub async fn modify(&self, id: &str, add: &[&str], remove: &[&str]) -> Result<Message> {
        let body = serde_json::json!({ "addLabelIds": add, "removeLabelIds": remove });
        self.send(self.http.post(format!("{BASE}/messages/{id}/modify")).json(&body))
            .await
    }
}
