//! Local-first integrations. Each one talks only to the service the user
//! chose, with a token they pasted, and only when they turned it on.
//!
//! - Calendar: the Mac's own Calendar (EventKit) through `lane-calendar`.
//! - Notion: a personal integration token; pages are imported into the
//!   file index (searchable, askable) and briefings can be exported.
//! - Obsidian and any Markdown app: the Markdown export folder plus the
//!   file index (see settings), no code needed.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// ── Calendar ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub calendar: String,
    #[serde(default)]
    pub attendees: Vec<String>,
    #[serde(default)]
    pub organizer: String,
    /// What to ask Lane to prepare for it (filled in after parsing).
    #[serde(default)]
    pub question: String,
}

pub fn calendar_helper(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(r) = resource_dir {
        candidates.push(r.join("audio").join("lane-calendar"));
    }
    candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/audio/lane-calendar")));
    candidates.into_iter().find(|p| p.is_file())
}

/// Run a helper and collect stdout, killing it after `timeout`. A helper
/// waiting on a permission prompt must never hang the app.
pub fn run_with_timeout(helper: &Path, arg: &str, timeout: Duration) -> Result<String, String> {
    let mut child = Command::new(helper).arg(arg).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn().map_err(|e| format!("helper: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("helper: no stdout")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut buf);
        let _ = tx.send(buf);
    });
    match rx.recv_timeout(timeout) {
        Ok(text) => {
            let _ = child.wait();
            Ok(text)
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err("helper did not answer in time (a permission prompt may be waiting)".into())
        }
    }
}

/// Events in the next `hours`. Err carries a user-facing reason.
pub fn upcoming_events(helper: &Path, hours: u32) -> Result<Vec<Event>, String> {
    let text = run_with_timeout(helper, &hours.to_string(), Duration::from_secs(150))?;
    let v: Value = serde_json::from_str(text.trim()).map_err(|_| "calendar access not granted".to_string())?;
    if let Some(e) = v["error"].as_str() {
        return Err(e.to_string());
    }
    let mut events: Vec<Event> = serde_json::from_value(v["events"].clone()).unwrap_or_default();
    for e in &mut events {
        e.question = prep_question(e);
    }
    Ok(events)
}

/// Events that ended in the last `hours` (the helper is called with a
/// negative span).
pub fn past_events(helper: &Path, hours: u32) -> Result<Vec<Event>, String> {
    let text = run_with_timeout(helper, &format!("-{hours}"), Duration::from_secs(20))?;
    let v: Value = serde_json::from_str(text.trim()).map_err(|_| "calendar access not granted".to_string())?;
    if let Some(e) = v["error"].as_str() {
        return Err(e.to_string());
    }
    Ok(serde_json::from_value(v["events"].clone()).unwrap_or_default())
}

/// The question Ask answers to prepare for an event.
pub fn prep_question(e: &Event) -> String {
    let who = if e.attendees.is_empty() { String::new() } else { format!(" with {}", e.attendees.join(", ")) };
    format!(
        "Prepare me for \"{}\"{}. What do I know about the people and the topic, what did we last discuss or decide, and what is still open?",
        e.title, who
    )
}

// ── Notion ───────────────────────────────────────────────────────────────

pub const NOTION_SERVICE: &str = "so.lane.app.notion";
const NOTION: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2022-06-28";

fn notion_req(method: &str, path: &str, token: &str) -> ureq::Request {
    let url = format!("{NOTION}{path}");
    let r = match method {
        "POST" => ureq::post(&url),
        "PATCH" => ureq::patch(&url),
        _ => ureq::get(&url),
    };
    r.set("Authorization", &format!("Bearer {token}")).set("Notion-Version", NOTION_VERSION).timeout(Duration::from_secs(30))
}

fn rich_text(v: &Value) -> String {
    v.as_array().map(|a| a.iter().filter_map(|t| t["plain_text"].as_str()).collect::<Vec<_>>().join("")).unwrap_or_default()
}

fn page_title(page: &Value) -> String {
    page["properties"]
        .as_object()
        .and_then(|props| props.values().find(|p| p["type"] == "title"))
        .map(|p| rich_text(&p["title"]))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Untitled".into())
}

/// Check a token: the bot user it belongs to, and its workspace name.
pub fn notion_whoami(token: &str) -> Result<String, String> {
    let v: Value = notion_req("GET", "/users/me", token)
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(401, _) => "Notion did not accept that token".to_string(),
            other => format!("Notion: {other}"),
        })?
        .into_json()
        .map_err(|e| e.to_string())?;
    let ws = v["bot"]["workspace_name"].as_str().unwrap_or("");
    let name = v["name"].as_str().unwrap_or("");
    Ok(if ws.is_empty() { name.to_string() } else { ws.to_string() })
}

#[derive(Debug, Clone)]
pub struct NotionPage {
    pub id: String,
    pub title: String,
    pub edited_ms: i64,
}

/// Every page the integration can see, newest edits first.
pub fn notion_pages(token: &str) -> Result<Vec<NotionPage>, String> {
    let mut pages = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut body = json!({"filter": {"property": "object", "value": "page"}, "sort": {"direction": "descending", "timestamp": "last_edited_time"}, "page_size": 100});
        if let Some(c) = &cursor {
            body["start_cursor"] = json!(c);
        }
        let resp = notion_req("POST", "/search", token).send_json(body).map_err(|e| format!("Notion: {e}"))?;
        let v: Value = resp.into_json().map_err(|e| e.to_string())?;
        for p in v["results"].as_array().unwrap_or(&vec![]) {
            let edited = p["last_edited_time"].as_str().and_then(parse_iso_ms).unwrap_or(0);
            pages.push(NotionPage { id: p["id"].as_str().unwrap_or("").to_string(), title: page_title(p), edited_ms: edited });
        }
        cursor = v["next_cursor"].as_str().map(String::from);
        if !v["has_more"].as_bool().unwrap_or(false) || pages.len() >= 2000 {
            break;
        }
    }
    Ok(pages)
}

/// Plain text of a page: its blocks, one paragraph per line, nested once.
pub fn notion_page_text(token: &str, page_id: &str) -> Result<String, String> {
    let mut out = String::new();
    blocks_text(token, page_id, 0, &mut out)?;
    Ok(out)
}

fn blocks_text(token: &str, block_id: &str, depth: usize, out: &mut String) -> Result<(), String> {
    if depth > 2 || out.len() > 60_000 {
        return Ok(());
    }
    let mut cursor: Option<String> = None;
    loop {
        let path = match &cursor {
            Some(c) => format!("/blocks/{block_id}/children?page_size=100&start_cursor={c}"),
            None => format!("/blocks/{block_id}/children?page_size=100"),
        };
        let v: Value = notion_req("GET", &path, token).call().map_err(|e| format!("Notion: {e}"))?.into_json().map_err(|e| e.to_string())?;
        for b in v["results"].as_array().unwrap_or(&vec![]) {
            let kind = b["type"].as_str().unwrap_or("");
            let text = rich_text(&b[kind]["rich_text"]);
            if !text.trim().is_empty() {
                let prefix = match kind {
                    "heading_1" | "heading_2" | "heading_3" => "# ",
                    "bulleted_list_item" | "numbered_list_item" | "to_do" => "- ",
                    _ => "",
                };
                out.push_str(prefix);
                out.push_str(text.trim());
                out.push('\n');
            }
            if b["has_children"].as_bool().unwrap_or(false) {
                if let Some(id) = b["id"].as_str() {
                    blocks_text(token, id, depth + 1, out)?;
                }
            }
        }
        cursor = v["next_cursor"].as_str().map(String::from);
        if !v["has_more"].as_bool().unwrap_or(false) {
            break;
        }
    }
    Ok(())
}

/// Create a page with plain paragraphs under a parent page.
pub fn notion_create_page(token: &str, parent_page_id: &str, title: &str, text: &str) -> Result<String, String> {
    let children: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(100)
        .map(|l| {
            let l = l.trim_start_matches("# ").trim_start_matches("- ");
            json!({"object": "block", "type": "paragraph", "paragraph": {"rich_text": [{"type": "text", "text": {"content": l.chars().take(2000).collect::<String>()}}]}})
        })
        .collect();
    let body = json!({
        "parent": {"page_id": parent_page_id},
        "properties": {"title": {"title": [{"type": "text", "text": {"content": title}}]}},
        "children": children
    });
    let v: Value = notion_req("POST", "/pages", token).send_json(body).map_err(|e| format!("Notion: {e}"))?.into_json().map_err(|e| e.to_string())?;
    v["url"].as_str().map(String::from).ok_or_else(|| "Notion did not return a page".into())
}

/// "2026-09-18T07:12:29.000Z" → ms. Enough of ISO 8601 for Notion.
pub fn parse_iso_ms(s: &str) -> Option<i64> {
    let (date, rest) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let time: String = rest.chars().take_while(|c| *c != 'Z' && *c != '+' && *c != '-').collect();
    let mut t = time.split(':');
    let (h, mi) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let sec = t.next().and_then(|x| x.split('.').next()).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
    // Days from civil (Howard Hinnant).
    let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400) + h * 3600 + mi * 60 + sec) * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_parse_matches_known_epoch() {
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(parse_iso_ms("2026-09-18T07:12:29.000Z"), Some(1_789_715_549_000));
        assert!(parse_iso_ms("garbage").is_none());
    }

    #[test]
    fn prep_question_names_attendees() {
        let e = Event { id: "1".into(), title: "Tender review".into(), start: 0, end: 0, all_day: false, location: String::new(), notes: String::new(), calendar: String::new(), attendees: vec!["Sarah Khan".into()], organizer: String::new(), question: String::new() };
        let q = prep_question(&e);
        assert!(q.contains("Tender review") && q.contains("Sarah Khan"));
    }

    #[test]
    fn page_title_reads_title_property() {
        let p = json!({"properties": {"Name": {"type": "title", "title": [{"plain_text": "Q3 "}, {"plain_text": "plan"}]}}});
        assert_eq!(page_title(&p), "Q3 plan");
        assert_eq!(page_title(&json!({"properties": {}})), "Untitled");
    }
}
