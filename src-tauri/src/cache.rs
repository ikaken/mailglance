//! Per-account SQLite cache: labels, message summaries, bodies and the last list of each view.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::error::Result;
use crate::gmail::Message;

pub struct Cache {
    conn: Connection,
}

#[derive(Serialize, Clone)]
pub struct LabelRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub unread: i64,
    pub total: i64,
}

#[derive(Serialize, Clone)]
pub struct Summary {
    pub id: String,
    pub thread_id: String,
    /// Milliseconds since the epoch.
    pub date: i64,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub snippet: String,
    pub label_ids: Vec<String>,
}

impl Summary {
    pub fn from_message(m: &Message) -> Summary {
        let header = |name: &str| {
            m.payload
                .as_ref()
                .and_then(|p| p.header(name))
                .unwrap_or_default()
                .to_string()
        };
        Summary {
            id: m.id.clone(),
            thread_id: m.thread_id.clone(),
            date: m.internal_date.as_deref().and_then(|d| d.parse().ok()).unwrap_or(0),
            from: header("From"),
            to: header("To"),
            subject: header("Subject"),
            snippet: decode_entities(m.snippet.as_deref().unwrap_or_default()),
            label_ids: m.label_ids.clone().unwrap_or_default(),
        }
    }
}

/// Snippets come HTML-escaped from the API.
fn decode_entities(s: &str) -> String {
    s.replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

impl Cache {
    pub fn open(path: &Path) -> Result<Cache> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS labels(
                 id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL,
                 unread INTEGER NOT NULL, total INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS messages(
                 id TEXT PRIMARY KEY, thread_id TEXT NOT NULL, date INTEGER NOT NULL,
                 sender TEXT NOT NULL, recipient TEXT NOT NULL, subject TEXT NOT NULL,
                 snippet TEXT NOT NULL, label_ids TEXT NOT NULL, body TEXT);
             CREATE TABLE IF NOT EXISTS lists(key TEXT PRIMARY KEY, ids TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        Ok(Cache { conn })
    }

    pub fn labels(&self) -> Result<Vec<LabelRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, kind, unread, total FROM labels ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok(LabelRow {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                unread: r.get(3)?,
                total: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn replace_labels(&mut self, labels: &[LabelRow]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM labels", [])?;
        for l in labels {
            tx.execute(
                "INSERT INTO labels(id, name, kind, unread, total) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![l.id, l.name, l.kind, l.unread, l.total],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Inserts or updates a summary while keeping a cached body.
    pub fn upsert(&self, s: &Summary) -> Result<()> {
        self.conn.execute(
            "INSERT INTO messages(id, thread_id, date, sender, recipient, subject, snippet, label_ids)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                 thread_id = excluded.thread_id, date = excluded.date, sender = excluded.sender,
                 recipient = excluded.recipient, subject = excluded.subject,
                 snippet = excluded.snippet, label_ids = excluded.label_ids",
            params![
                s.id,
                s.thread_id,
                s.date,
                s.from,
                s.to,
                s.subject,
                s.snippet,
                serde_json::to_string(&s.label_ids)?
            ],
        )?;
        Ok(())
    }

    pub fn set_label_ids(&self, id: &str, label_ids: &[String]) -> Result<()> {
        self.conn.execute(
            "UPDATE messages SET label_ids = ?2 WHERE id = ?1",
            params![id, serde_json::to_string(label_ids)?],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM messages WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_messages(&self) -> Result<()> {
        self.conn.execute_batch("DELETE FROM messages; DELETE FROM lists;")?;
        Ok(())
    }

    pub fn known_ids(&self, ids: &[String]) -> Result<HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT 1 FROM messages WHERE id = ?1")?;
        let mut known = HashSet::new();
        for id in ids {
            if stmt.exists([id])? {
                known.insert(id.clone());
            }
        }
        Ok(known)
    }

    pub fn summary(&self, id: &str) -> Result<Option<Summary>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, thread_id, date, sender, recipient, subject, snippet, label_ids
                 FROM messages WHERE id = ?1",
                [id],
                |r| {
                    let label_ids: String = r.get(7)?;
                    Ok(Summary {
                        id: r.get(0)?,
                        thread_id: r.get(1)?,
                        date: r.get(2)?,
                        from: r.get(3)?,
                        to: r.get(4)?,
                        subject: r.get(5)?,
                        snippet: r.get(6)?,
                        label_ids: serde_json::from_str(&label_ids).unwrap_or_default(),
                    })
                },
            )
            .optional()?)
    }

    pub fn set_list(&self, key: &str, ids: &[String]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO lists(key, ids) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET ids = excluded.ids",
            params![key, serde_json::to_string(ids)?],
        )?;
        Ok(())
    }

    /// Summaries of a view in its last known order.
    pub fn list(&self, key: &str) -> Result<Vec<Summary>> {
        let ids: Option<String> = self
            .conn
            .query_row("SELECT ids FROM lists WHERE key = ?1", [key], |r| r.get(0))
            .optional()?;
        let ids: Vec<String> = ids.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(s) = self.summary(&id)? {
                out.push(s);
            }
        }
        Ok(out)
    }

    pub fn body(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT body FROM messages WHERE id = ?1", [id], |r| r.get(0))
            .optional()?
            .flatten())
    }

    pub fn set_body(&self, id: &str, body: &str) -> Result<()> {
        self.conn
            .execute("UPDATE messages SET body = ?2 WHERE id = ?1", params![id, body])?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}
