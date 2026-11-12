//! Turns a `format=full` message into one HTML document for display.

use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};

use crate::error::Result;
use crate::gmail::{Gmail, Message, Part};

/// Inline images larger than this in total are left as broken `cid:` links.
const INLINE_IMAGE_BUDGET: usize = 10 * 1024 * 1024;

pub struct Attachment {
    pub filename: String,
    pub size: i64,
}

struct Collected<'a> {
    html: Option<&'a Part>,
    text: Option<&'a Part>,
    inline: Vec<(String, &'a Part)>,
    attachments: Vec<Attachment>,
    /// Every part that carries a file, including inline images that have a filename.
    files: Vec<&'a Part>,
}

fn collect<'a>(part: &'a Part, out: &mut Collected<'a>) {
    let mime = part.mime_type.as_deref().unwrap_or_default().to_ascii_lowercase();
    let filename = part.filename.as_deref().unwrap_or_default();
    if let Some(children) = &part.parts {
        for child in children {
            collect(child, out);
        }
        return;
    }
    if let Some(cid) = part.header("Content-ID") {
        let cid = cid.trim().trim_start_matches('<').trim_end_matches('>').to_string();
        out.inline.push((cid, part));
    }
    if !filename.is_empty() {
        out.files.push(part);
        out.attachments.push(Attachment {
            filename: filename.to_string(),
            size: part.body.as_ref().and_then(|b| b.size).unwrap_or(0),
        });
    } else if mime == "text/html" && out.html.is_none() {
        out.html = Some(part);
    } else if mime == "text/plain" && out.text.is_none() {
        out.text = Some(part);
    }
}

fn decode_b64(data: &str) -> Vec<u8> {
    let trimmed = data.trim_end_matches('=');
    URL_SAFE_NO_PAD
        .decode(trimmed)
        .or_else(|_| URL_SAFE.decode(data))
        .unwrap_or_default()
}

fn charset(part: &Part) -> Option<String> {
    let ct = part.header("Content-Type")?;
    ct.split(';').skip(1).find_map(|p| {
        let (k, v) = p.split_once('=')?;
        k.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| v.trim().trim_matches('"').to_string())
    })
}

/// Gmail usually hands back UTF-8, but some parts keep their original charset
/// (Shift_JIS, ISO-2022-JP). Valid UTF-8 with non-ASCII is trusted as UTF-8;
/// otherwise the declared charset decides.
pub fn decode_text(bytes: &[u8], declared: Option<&str>) -> String {
    if let Ok(s) = std::str::from_utf8(bytes)
        && (!s.is_ascii() || declared.is_none())
    {
        return s.to_string();
    }
    let encoding = declared
        .and_then(|c| encoding_rs::Encoding::for_label(c.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode(bytes).0.into_owned()
}

fn part_text(part: &Part) -> String {
    let data = part.body.as_ref().and_then(|b| b.data.as_deref()).unwrap_or_default();
    decode_text(&decode_b64(data), charset(part).as_deref())
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Escapes plain text and turns bare http(s) URLs into links.
fn text_to_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    let mut rest = text;
    while let Some(start) = ["https://", "http://"].iter().filter_map(|p| rest.find(p)).min() {
        out.push_str(&escape_html(&rest[..start]));
        let tail = &rest[start..];
        let end = tail
            .find(|c: char| c.is_whitespace() || "<>\"'「」（）()、。".contains(c))
            .unwrap_or(tail.len());
        let url = escape_html(&tail[..end]);
        out.push_str(&format!("<a href=\"{url}\">{url}</a>"));
        rest = &tail[end..];
    }
    out.push_str(&escape_html(rest));
    format!(
        "<pre style=\"white-space:pre-wrap;word-break:break-word;font-family:inherit;margin:0\">{out}</pre>"
    )
}

pub struct Rendered {
    pub html: String,
    pub attachments: Vec<Attachment>,
}

pub async fn render(gmail: &Gmail<'_>, msg: &Message) -> Result<Rendered> {
    let Some(payload) = &msg.payload else {
        return Ok(Rendered { html: String::new(), attachments: vec![] });
    };
    let mut c = Collected { html: None, text: None, inline: vec![], attachments: vec![], files: vec![] };
    collect(payload, &mut c);

    let mut html = match (c.html, c.text) {
        (Some(p), _) => part_text(p),
        (None, Some(p)) => text_to_html(&part_text(p)),
        (None, None) => String::new(),
    };

    // Embed cid: images as data URIs so the body works offline and without remote loads.
    let mut budget = INLINE_IMAGE_BUDGET;
    for (cid, part) in &c.inline {
        let needle = format!("cid:{cid}");
        if cid.is_empty() || !html.contains(&needle) {
            continue;
        }
        let body = part.body.as_ref();
        let data = match (body.and_then(|b| b.data.clone()), body.and_then(|b| b.attachment_id.as_deref())) {
            (Some(d), _) => d,
            (None, Some(att)) => match gmail.attachment(&msg.id, att).await {
                Ok(d) => d,
                Err(_) => continue,
            },
            _ => continue,
        };
        let bytes = decode_b64(&data);
        if bytes.len() > budget {
            continue;
        }
        budget -= bytes.len();
        let mime = part.mime_type.as_deref().unwrap_or("application/octet-stream");
        let uri = format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        html = html.replace(&needle, &uri);
    }

    Ok(Rendered { html, attachments: c.attachments })
}

/// What a forward needs from the original: its HTML as sent (with `cid:` references) and files.
pub struct ForwardContent {
    pub html: String,
    pub files: Vec<FilePart>,
}

pub struct FilePart {
    pub filename: String,
    pub mime: String,
    /// Set for images the HTML shows inline; they stay inline in the forward.
    pub content_id: Option<String>,
    pub data: Vec<u8>,
}

/// Gmail refuses messages above 25 MB.
const FORWARD_LIMIT: i64 = 25 * 1024 * 1024;

pub async fn forward_content(gmail: &Gmail<'_>, msg: &Message) -> Result<ForwardContent> {
    let Some(payload) = &msg.payload else {
        return Ok(ForwardContent { html: String::new(), files: vec![] });
    };
    let mut c = Collected { html: None, text: None, inline: vec![], attachments: vec![], files: vec![] };
    collect(payload, &mut c);
    let html = match (c.html, c.text) {
        (Some(p), _) => part_text(p),
        (None, Some(p)) => text_to_html(&part_text(p)),
        (None, None) => String::new(),
    };

    let total: i64 = c.attachments.iter().map(|a| a.size).sum();
    if total > FORWARD_LIMIT {
        return Err(crate::error::Error::new(
            "添付ファイルの合計が 25 MB を超えるため、転送できません。Gmail の画面から転送してください。",
        ));
    }

    let mut files = Vec::new();
    for part in c.files {
        let body = part.body.as_ref();
        let data = match (body.and_then(|b| b.data.clone()), body.and_then(|b| b.attachment_id.as_deref())) {
            (Some(d), _) => d,
            (None, Some(att)) => gmail.attachment(&msg.id, att).await?,
            _ => continue,
        };
        let content_id = part
            .header("Content-ID")
            .map(|c| c.trim().trim_start_matches('<').trim_end_matches('>').to_string())
            .filter(|cid| !cid.is_empty() && html.contains(&format!("cid:{cid}")));
        files.push(FilePart {
            filename: part.filename.clone().unwrap_or_default(),
            mime: part.mime_type.clone().unwrap_or_else(|| "application/octet-stream".into()),
            content_id,
            data: decode_b64(&data),
        });
    }
    Ok(ForwardContent { html, files })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linkifies_plain_text() {
        let html = text_to_html("see https://example.com/a?b=1&c=2 now <x>");
        assert!(html.contains("<a href=\"https://example.com/a?b=1&amp;c=2\">"));
        assert!(html.contains("&lt;x&gt;"));
    }

    #[test]
    fn decodes_shift_jis() {
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode("日本語");
        assert_eq!(decode_text(&bytes, Some("Shift_JIS")), "日本語");
    }

    #[test]
    fn decodes_iso_2022_jp() {
        let (bytes, _, _) = encoding_rs::ISO_2022_JP.encode("こんにちは");
        assert_eq!(decode_text(&bytes, Some("iso-2022-jp")), "こんにちは");
    }

    #[test]
    fn trusts_utf8_even_if_declared_otherwise() {
        assert_eq!(decode_text("日本語".as_bytes(), Some("iso-2022-jp")), "日本語");
    }
}
