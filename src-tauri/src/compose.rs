//! Reply / reply-all / forward drafts. The app creates a draft through the Gmail API (threaded
//! for replies) and Gmail's own web window opens it for editing and sending.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::Deserialize;

use crate::body::{FilePart, escape_html};

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Reply,
    ReplyAll,
    Forward,
}

/// Header values of the message being answered.
#[derive(Default, Clone)]
pub struct Original {
    pub from: String,
    pub to: String,
    pub cc: String,
    pub reply_to: String,
    pub subject: String,
    pub date: String,
    pub message_id: String,
    pub references: String,
}

#[derive(Debug, PartialEq)]
pub struct Plan {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    /// Replies stay in the original thread; a forward starts a new one, as in Gmail.
    pub threaded: bool,
}

/// Splits an address list on commas that are outside quotes and `<...>`.
pub fn split_addresses(list: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut cur, mut quoted, mut angle) = (String::new(), false, false);
    let mut chars = list.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' if quoted => {
                cur.push(c);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
                continue;
            }
            '"' => quoted = !quoted,
            '<' if !quoted => angle = true,
            '>' if !quoted => angle = false,
            ',' if !quoted && !angle => {
                push_trimmed(&mut out, &cur);
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    push_trimmed(&mut out, &cur);
    out
}

fn push_trimmed(out: &mut Vec<String>, s: &str) {
    let s = s.trim();
    if !s.is_empty() {
        out.push(s.to_string());
    }
}

/// The bare, lowercased address of `Name <a@b>` or `a@b`.
pub fn address_of(s: &str) -> String {
    let inner = match (s.rfind('<'), s.rfind('>')) {
        (Some(a), Some(b)) if a < b => &s[a + 1..b],
        _ => s,
    };
    inner.trim().to_ascii_lowercase()
}

/// Adds `items` to `list` unless the address is `me` or already present.
fn add_unique(list: &mut Vec<String>, items: Vec<String>, me: &str, seen: &mut Vec<String>) {
    for item in items {
        let addr = address_of(&item);
        if addr == me || seen.contains(&addr) {
            continue;
        }
        seen.push(addr);
        list.push(item);
    }
}

fn with_prefix(subject: &str, prefixes: &[&str], add: &str) -> String {
    let s = subject.trim();
    let lower = s.to_ascii_lowercase();
    if prefixes.iter().any(|p| lower.starts_with(p)) {
        s.to_string()
    } else {
        format!("{add}{s}")
    }
}

pub fn plan(kind: Kind, me: &str, o: &Original) -> Plan {
    let me = me.to_ascii_lowercase();
    if kind == Kind::Forward {
        return Plan {
            to: vec![],
            cc: vec![],
            subject: with_prefix(&o.subject, &["fwd:", "fw:"], "Fwd: "),
            in_reply_to: None,
            references: None,
            threaded: false,
        };
    }

    let sent_by_me = address_of(&o.from) == me;
    // Answering a message of one's own goes to its recipients, as in Gmail.
    let target = if sent_by_me {
        split_addresses(&o.to)
    } else if !o.reply_to.trim().is_empty() {
        split_addresses(&o.reply_to)
    } else {
        split_addresses(&o.from)
    };

    let (mut to, mut cc, mut seen) = (Vec::new(), Vec::new(), Vec::new());
    add_unique(&mut to, target, &me, &mut seen);
    if kind == Kind::ReplyAll {
        if !sent_by_me {
            add_unique(&mut to, split_addresses(&o.to), &me, &mut seen);
        }
        add_unique(&mut cc, split_addresses(&o.cc), &me, &mut seen);
    }

    let id = o.message_id.trim();
    let references = match (o.references.trim(), id) {
        ("", "") => None,
        ("", id) => Some(id.to_string()),
        (r, "") => Some(r.to_string()),
        (r, id) => Some(format!("{r} {id}")),
    };
    Plan {
        to,
        cc,
        subject: with_prefix(&o.subject, &["re:"], "Re: "),
        in_reply_to: (!id.is_empty()).then(|| id.to_string()),
        references,
        threaded: true,
    }
}

/// HTML body: an empty line to type in, then the quoted original.
pub fn quote_html(kind: Kind, o: &Original, html: &str) -> String {
    // A huge inline-image body would blow past the API's request limit.
    let html = if html.len() > 3_000_000 { "（本文が大きいため引用を省略しました）" } else { html };
    let from = escape_html(&o.from);
    if kind == Kind::Forward {
        format!(
            "<div dir=\"ltr\"><br></div><br><div class=\"gmail_quote\"><div dir=\"ltr\" class=\"gmail_attr\">\
             ---------- Forwarded message ---------<br>From: {from}<br>Date: {}<br>Subject: {}<br>To: {}<br></div><br><br>{html}</div>",
            escape_html(&o.date),
            escape_html(&o.subject),
            escape_html(&o.to),
        )
    } else {
        format!(
            "<div dir=\"ltr\"><br></div><br><div class=\"gmail_quote\"><div dir=\"ltr\" class=\"gmail_attr\">{} {from}:<br></div>\
             <blockquote class=\"gmail_quote\" style=\"margin:0px 0px 0px 0.8ex;border-left:1px solid rgb(204,204,204);padding-left:1ex\">{html}</blockquote></div>",
            escape_html(&o.date),
        )
    }
}

/// Header values must stay on one line: a stray CR/LF would let message text inject headers.
fn one_line(s: &str) -> String {
    s.split(['\r', '\n']).map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

/// RFC 2047 encoding of a subject that has non-ASCII characters.
fn encode_subject(s: &str) -> String {
    let s = one_line(s);
    if s.is_ascii() {
        return s;
    }
    encode_words(&s)
}

/// `Name <a@b>` with a non-ASCII name becomes `=?UTF-8?B?...?= <a@b>`. Raw UTF-8 in an address
/// header is read as Latin-1 by Gmail and comes out garbled.
fn encode_mailbox(s: &str) -> String {
    let s = one_line(s);
    if s.is_ascii() {
        return s;
    }
    match s.rfind('<') {
        Some(lt) => {
            let name = s[..lt].trim().trim_matches('"').replace("\\\"", "\"");
            let addr = &s[lt..];
            if name.is_empty() { addr.to_string() } else { format!("{} {addr}", encode_words(&name)) }
        }
        None => s,
    }
}

fn encode_words(s: &str) -> String {
    // An encoded word is at most 75 characters; 45 bytes of text become 60 of base64.
    let mut words = Vec::new();
    let mut chunk = String::new();
    for c in s.chars() {
        if chunk.len() + c.len_utf8() > 45 {
            words.push(std::mem::take(&mut chunk));
        }
        chunk.push(c);
    }
    if !chunk.is_empty() {
        words.push(chunk);
    }
    words
        .iter()
        .map(|w| format!("=?UTF-8?B?{}?=", STANDARD.encode(w)))
        .collect::<Vec<_>>()
        .join("\r\n ")
}

fn wrap76(s: &str) -> String {
    s.as_bytes()
        .chunks(76)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// A value for a `filename="..."` parameter. Non-ASCII names go out as an RFC 2047 encoded
/// word, which Gmail understands in filenames.
fn quoted_string(s: &str) -> String {
    let s = one_line(s);
    let s = if s.is_ascii() { s } else { format!("=?UTF-8?B?{}?=", STANDARD.encode(&s)) };
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// `build_mime` without files, base64url-encoded as the Gmail API's JSON form wants it.
pub fn build_raw(plan: &Plan, html: &str) -> String {
    URL_SAFE_NO_PAD.encode(build_mime(plan, html, &[]))
}

/// The draft as an RFC 822 message. With files it is multipart/mixed: the HTML first, then each
/// file (images the HTML shows inline keep their Content-ID).
pub fn build_mime(plan: &Plan, html: &str, files: &[FilePart]) -> String {
    let mut m = String::new();
    let mut header = |name: &str, value: &str| {
        m.push_str(name);
        m.push_str(": ");
        m.push_str(value);
        m.push_str("\r\n");
    };
    let mailboxes = |list: &[String]| list.iter().map(|a| encode_mailbox(a)).collect::<Vec<_>>().join(", ");
    if !plan.to.is_empty() {
        header("To", &mailboxes(&plan.to));
    }
    if !plan.cc.is_empty() {
        header("Cc", &mailboxes(&plan.cc));
    }
    header("Subject", &encode_subject(&plan.subject));
    if let Some(v) = &plan.in_reply_to {
        header("In-Reply-To", &one_line(v));
    }
    if let Some(v) = &plan.references {
        header("References", &one_line(v));
    }
    header("MIME-Version", "1.0");
    let html_part = |m: &mut String| {
        m.push_str("Content-Type: text/html; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
        m.push_str(&wrap76(&STANDARD.encode(html)));
        m.push_str("\r\n");
    };
    if files.is_empty() {
        html_part(&mut m);
        return m;
    }

    let boundary = format!("=_mailglance_{:016x}{:016x}", rand::random::<u64>(), rand::random::<u64>());
    m.push_str(&format!("Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n"));
    m.push_str(&format!("--{boundary}\r\n"));
    html_part(&mut m);
    for f in files {
        let name = quoted_string(&f.filename);
        let mime = one_line(&f.mime);
        m.push_str(&format!("--{boundary}\r\nContent-Type: {mime}; name=\"{name}\"\r\n"));
        match &f.content_id {
            Some(cid) => m.push_str(&format!(
                "Content-Disposition: inline; filename=\"{name}\"\r\nContent-ID: <{}>\r\n",
                one_line(cid)
            )),
            None => m.push_str(&format!("Content-Disposition: attachment; filename=\"{name}\"\r\n")),
        }
        m.push_str("Content-Transfer-Encoding: base64\r\n\r\n");
        m.push_str(&wrap76(&STANDARD.encode(&f.data)));
        m.push_str("\r\n");
    }
    m.push_str(&format!("--{boundary}--\r\n"));
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn orig() -> Original {
        Original {
            from: "Taro Yamada <taro@example.com>".into(),
            to: "me@example.com, \"Doe, Jane\" <jane@example.com>".into(),
            cc: "Bob <bob@example.com>, ME@example.com".into(),
            subject: "会議の件".into(),
            date: "Thu, 12 Nov 2026 10:00:00 +0900".into(),
            message_id: "<a@x>".into(),
            references: "<r@x>".into(),
            ..Default::default()
        }
    }

    #[test]
    fn splits_on_top_level_commas_only() {
        let v = split_addresses("\"Doe, Jane\" <j@x.com>, b@x.com ,<c@x.com>");
        assert_eq!(v, vec!["\"Doe, Jane\" <j@x.com>", "b@x.com", "<c@x.com>"]);
    }

    #[test]
    fn extracts_addresses() {
        assert_eq!(address_of("Taro <Taro@Example.com>"), "taro@example.com");
        assert_eq!(address_of(" a@b.c "), "a@b.c");
    }

    #[test]
    fn reply_goes_to_sender_and_threads() {
        let p = plan(Kind::Reply, "me@example.com", &orig());
        assert_eq!(p.to, vec!["Taro Yamada <taro@example.com>"]);
        assert!(p.cc.is_empty());
        assert_eq!(p.subject, "Re: 会議の件");
        assert_eq!(p.in_reply_to.as_deref(), Some("<a@x>"));
        assert_eq!(p.references.as_deref(), Some("<r@x> <a@x>"));
        assert!(p.threaded);
    }

    #[test]
    fn reply_prefers_reply_to() {
        let mut o = orig();
        o.reply_to = "list@example.com".into();
        assert_eq!(plan(Kind::Reply, "me@example.com", &o).to, vec!["list@example.com"]);
    }

    #[test]
    fn reply_all_drops_me_and_duplicates() {
        let p = plan(Kind::ReplyAll, "me@example.com", &orig());
        assert_eq!(p.to, vec!["Taro Yamada <taro@example.com>", "\"Doe, Jane\" <jane@example.com>"]);
        assert_eq!(p.cc, vec!["Bob <bob@example.com>"]);
    }

    #[test]
    fn answering_own_message_goes_to_its_recipients() {
        let mut o = orig();
        o.from = "me@example.com".into();
        let p = plan(Kind::Reply, "me@example.com", &o);
        assert_eq!(p.to, vec!["\"Doe, Jane\" <jane@example.com>"]);
        let all = plan(Kind::ReplyAll, "me@example.com", &o);
        assert_eq!(all.to, vec!["\"Doe, Jane\" <jane@example.com>"]);
        assert_eq!(all.cc, vec!["Bob <bob@example.com>"]);
    }

    #[test]
    fn subject_prefix_is_not_doubled() {
        let mut o = orig();
        o.subject = "RE: hello".into();
        assert_eq!(plan(Kind::Reply, "me@x", &o).subject, "RE: hello");
        o.subject = "Fw: hello".into();
        assert_eq!(plan(Kind::Forward, "me@x", &o).subject, "Fw: hello");
    }

    #[test]
    fn forward_is_a_new_thread() {
        let p = plan(Kind::Forward, "me@example.com", &orig());
        assert!(p.to.is_empty() && p.cc.is_empty());
        assert_eq!(p.subject, "Fwd: 会議の件");
        assert!(!p.threaded && p.in_reply_to.is_none() && p.references.is_none());
    }

    #[test]
    fn raw_message_has_headers_and_decodable_body() {
        let p = plan(Kind::Reply, "me@example.com", &orig());
        let raw = build_raw(&p, "<p>こんにちは</p>");
        let bytes = URL_SAFE_NO_PAD.decode(raw).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("To: Taro Yamada <taro@example.com>\r\n"));
        assert!(text.contains("In-Reply-To: <a@x>\r\n"));
        assert!(text.contains("Subject: =?UTF-8?B?"));
        let body = text.split("\r\n\r\n").nth(1).unwrap().replace("\r\n", "");
        assert_eq!(String::from_utf8(STANDARD.decode(body).unwrap()).unwrap(), "<p>こんにちは</p>");
    }

    #[test]
    fn forward_attaches_files_and_keeps_inline_images_inline() {
        let p = plan(Kind::Forward, "me@example.com", &orig());
        let files = vec![
            FilePart {
                filename: "見積書.pdf".into(),
                mime: "application/pdf".into(),
                content_id: None,
                data: b"%PDF".to_vec(),
            },
            FilePart {
                filename: "logo.png".into(),
                mime: "image/png".into(),
                content_id: Some("c1".into()),
                data: vec![1, 2, 3],
            },
        ];
        let mime = build_mime(&p, "<img src=\"cid:c1\">", &files);
        assert!(mime.contains("Content-Type: multipart/mixed; boundary="));
        assert!(mime.contains("Content-Disposition: attachment; filename=\"=?UTF-8?B?"));
        assert!(mime.contains("Content-Disposition: inline; filename=\"logo.png\"\r\nContent-ID: <c1>"));
        assert!(mime.contains(&STANDARD.encode(b"%PDF")));
        assert!(mime.trim_end().ends_with("--"));
    }

    #[test]
    fn japanese_display_names_are_encoded_in_address_headers() {
        let mut o = orig();
        o.from = "\"山田 太郎\" <taro@example.com>".into();
        o.cc = "鈴木 <suzuki@example.com>".into();
        let p = plan(Kind::ReplyAll, "me@example.com", &o);
        let raw = build_mime(&p, "x", &[]);
        let head = raw.split("\r\n\r\n").next().unwrap();
        assert!(head.is_ascii(), "headers must be pure ASCII: {head}");
        let to = head.lines().find(|l| l.starts_with("To: ")).unwrap();
        assert!(to.contains("=?UTF-8?B?") && to.contains("<taro@example.com>"));
        let decoded = STANDARD.decode(to.split("=?UTF-8?B?").nth(1).unwrap().split("?=").next().unwrap()).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "山田 太郎");
        assert!(head.contains("<suzuki@example.com>"));
    }

    #[test]
    fn filename_quotes_are_escaped() {
        assert_eq!(quoted_string("a\"b.txt"), "a\\\"b.txt");
    }

    #[test]
    fn long_japanese_subject_is_split_into_short_words() {
        let s = encode_subject(&"あ".repeat(60));
        assert!(s.split("\r\n ").all(|w| w.len() <= 75));
        assert!(s.split("\r\n ").count() > 1);
    }

    #[test]
    fn header_injection_is_flattened() {
        let mut p = plan(Kind::Reply, "me@x", &orig());
        p.subject = "hi\r\nBcc: evil@x".into();
        let raw = build_raw(&p, "x");
        let text = String::from_utf8(URL_SAFE_NO_PAD.decode(raw).unwrap()).unwrap();
        assert!(!text.contains("\r\nBcc:"));
    }
}
