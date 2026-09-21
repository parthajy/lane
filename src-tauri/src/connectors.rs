//! Connectors: anything the user can point Lane at becomes memory, on a
//! schedule, on this machine, with the user's own keys.
//!
//! A connector is one JSON file in `<data dir>/connectors/`. Three kinds:
//!
//! - `http`: fetch a URL (headers may use `{{secret}}`), pick items out of
//!   the JSON with a small path, map fields to title/body/time/url.
//! - `script`: run any executable; each line of stdout is a JSON item.
//! - `mcp`: call one tool on an MCP server (stdio); the text it returns is
//!   the item body.
//!
//! Items go into the file index (`connector://<id>/<item>`), so they are
//! searchable and cited by Ask. With `"memories": true`, new or changed
//! items also become activities, and Rabbit makes memories of them: facts,
//! tasks, people. That is how something the user never opened themselves
//! (yesterday's analytics, a CRM change) shows up in their briefing.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    /// File stem; the id used in paths and the Keychain.
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// http | script | mcp
    pub kind: String,
    #[serde(default = "default_every")]
    pub every_minutes: u64,
    /// Also make memories of new or changed items (default: index only).
    #[serde(default)]
    pub memories: bool,

    // http
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<Value>,
    /// Where the items are in the response, e.g. "$.data[*]" or "$.rows".
    #[serde(default = "default_items")]
    pub items: String,

    // script
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,

    // mcp
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub arguments: Value,

    // field mapping: templates over each item, e.g. "{{name}} — {{status}}"
    #[serde(default = "t_id")]
    pub item_id: String,
    #[serde(default = "t_title")]
    pub item_title: String,
    #[serde(default = "t_body")]
    pub item_body: String,
    #[serde(default = "t_time")]
    pub item_time: String,
    #[serde(default = "t_url")]
    pub item_url: String,
}

fn default_every() -> u64 { 60 }
fn default_method() -> String { "GET".into() }
fn default_items() -> String { "$".into() }
fn t_id() -> String { "{{id}}".into() }
fn t_title() -> String { "{{title}}".into() }
fn t_body() -> String { "{{body}}".into() }
fn t_time() -> String { "{{time}}".into() }
fn t_url() -> String { "{{url}}".into() }

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub title: String,
    pub body: String,
    /// ms since epoch; 0 = unknown (the run time is used).
    pub time: i64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub every_minutes: u64,
    pub memories: bool,
    pub last_run: i64,
    pub items: i64,
    pub error: String,
    pub file: String,
}

pub fn secret_service(id: &str) -> String {
    format!("so.lane.app.connector.{id}")
}

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("connectors")
}

/// All specs in the folder, by file. Broken files are reported, not skipped silently.
pub fn load(dir: &Path) -> Vec<Result<Spec, (String, String)>> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if p.extension().map_or(true, |e| e != "json") || stem.starts_with('_') || stem.starts_with('.') {
            continue;
        }
        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Spec>(&t).map_err(|e| e.to_string())) {
            Ok(mut s) => {
                s.id = stem;
                if !["http", "script", "mcp"].contains(&s.kind.as_str()) {
                    out.push(Err((s.id.clone(), format!("kind must be http, script or mcp (got \"{}\")", s.kind))));
                } else {
                    out.push(Ok(s));
                }
            }
            Err(e) => out.push(Err((stem, e))),
        }
    }
    out
}

/// Write the folder's README and examples once, so people can start by copying.
pub fn ensure_examples(dir: &Path) {
    if dir.join("_README.md").is_file() {
        return;
    }
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join("_README.md"), README);
    let _ = std::fs::write(dir.join("_example-http.json.txt"), EXAMPLE_HTTP);
    let _ = std::fs::write(dir.join("_example-script.json.txt"), EXAMPLE_SCRIPT);
    let _ = std::fs::write(dir.join("_example-mcp.json.txt"), EXAMPLE_MCP);
}

// ── Running ──────────────────────────────────────────────────────────────

/// Run a connector once. `since` is the previous run (ms) for `{{since}}`.
pub fn run(spec: &Spec, secret: Option<&str>, since: i64) -> Result<Vec<Item>, String> {
    let now = crate::capture::now_ms();
    let globals = globals(secret, since);
    let raw: Vec<Value> = match spec.kind.as_str() {
        "http" => {
            let url = template(&spec.url, &Value::Null, &globals);
            let mut req = match spec.method.to_uppercase().as_str() {
                "POST" => ureq::post(&url),
                "PUT" => ureq::put(&url),
                _ => ureq::get(&url),
            }
            .timeout(Duration::from_secs(60));
            for (k, v) in &spec.headers {
                req = req.set(k, &template(v, &Value::Null, &globals));
            }
            let resp = match &spec.body {
                Some(b) => req.send_json(fill_value(b, &globals)),
                None => req.call(),
            }
            .map_err(|e| match e {
                ureq::Error::Status(code, _) => format!("HTTP {code}"),
                other => other.to_string(),
            })?;
            let v: Value = resp.into_json().map_err(|e| format!("not JSON: {e}"))?;
            select(&v, &spec.items)
        }
        "script" => {
            if spec.command.trim().is_empty() {
                return Err("script connector needs \"command\"".into());
            }
            let args: Vec<String> = spec.args.iter().map(|a| template(a, &Value::Null, &globals)).collect();
            let out = std::process::Command::new(&spec.command)
                .args(&args)
                .env("LANE_SINCE", since.to_string())
                .env("LANE_SECRET", secret.unwrap_or(""))
                .output()
                .map_err(|e| format!("could not run {}: {e}", spec.command))?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                return Err(format!("script exited with {}: {}", out.status, err.chars().take(300).collect::<String>()));
            }
            let text = String::from_utf8_lossy(&out.stdout);
            // Either one JSON document (array/object) or JSON lines.
            match serde_json::from_str::<Value>(text.trim()) {
                Ok(v) => select(&v, &spec.items),
                Err(_) => text.lines().filter(|l| !l.trim().is_empty()).filter_map(|l| serde_json::from_str::<Value>(l).ok()).collect(),
            }
        }
        "mcp" => {
            if spec.command.trim().is_empty() || spec.tool.trim().is_empty() {
                return Err("mcp connector needs \"command\" and \"tool\"".into());
            }
            let args = fill_value(&spec.arguments, &globals);
            let text = crate::mcp::call_tool(&spec.command, &spec.args, &spec.tool, args)?;
            // Tool text may itself be JSON; otherwise it is one item.
            match serde_json::from_str::<Value>(text.trim()) {
                Ok(v) if v.is_array() || v.is_object() => select(&v, &spec.items),
                _ => vec![serde_json::json!({"id": format!("{}-{}", spec.tool, crate::engine::day_of(now)), "title": format!("{} · {}", spec.name, crate::engine::day_of(now)), "body": text})],
            }
        }
        _ => return Err("unknown kind".into()),
    };
    let mut items = Vec::new();
    for (i, r) in raw.iter().enumerate().take(500) {
        let id = template(&spec.item_id, r, &globals);
        let title = template(&spec.item_title, r, &globals);
        let body = template(&spec.item_body, r, &globals);
        let time = template(&spec.item_time, r, &globals);
        let url = template(&spec.item_url, r, &globals);
        let body = if body.trim().is_empty() { pretty(r) } else { body };
        let title = if title.trim().is_empty() { body.lines().next().unwrap_or("Item").chars().take(80).collect() } else { title };
        let id = if id.trim().is_empty() { format!("{}-{}", crate::store::text_hash(&format!("{title}\n{body}")), i) } else { id };
        items.push(Item { id: safe_id(&id), title: title.trim().to_string(), body: body.trim().to_string(), time: parse_time(&time).unwrap_or(0), url });
    }
    Ok(items)
}

fn globals(secret: Option<&str>, since: i64) -> HashMap<String, String> {
    let mut g = HashMap::new();
    g.insert("secret".into(), secret.unwrap_or("").to_string());
    g.insert("since".into(), since.to_string());
    g.insert("since_iso".into(), if since > 0 { iso(since) } else { String::new() });
    g.insert("since_date".into(), if since > 0 { crate::engine::day_of(since) } else { String::new() });
    g.insert("today".into(), crate::engine::day_of(crate::capture::now_ms()));
    g.insert("yesterday".into(), crate::engine::day_of(crate::capture::now_ms() - 86_400_000));
    g
}

fn iso(ms: i64) -> String {
    // UTC, good enough for "since" parameters.
    let secs = ms / 1000;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `{{field}}`, `{{a.b}}`, `{{secret}}`, `{{since_iso}}` → text. Missing → "".
pub fn template(t: &str, item: &Value, globals: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = t;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let key = after[..end].trim();
        if let Some(g) = globals.get(key) {
            out.push_str(g);
        } else {
            out.push_str(&value_to_text(&get_path(item, key)));
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn fill_value(v: &Value, globals: &HashMap<String, String>) -> Value {
    match v {
        Value::String(s) => Value::String(template(s, &Value::Null, globals)),
        Value::Array(a) => Value::Array(a.iter().map(|x| fill_value(x, globals)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), fill_value(x, globals))).collect()),
        other => other.clone(),
    }
}

fn get_path(v: &Value, path: &str) -> Value {
    let mut cur = v;
    for part in path.split('.') {
        if part.is_empty() {
            continue;
        }
        cur = match cur {
            Value::Object(o) => o.get(part).unwrap_or(&Value::Null),
            Value::Array(a) => part.parse::<usize>().ok().and_then(|i| a.get(i)).unwrap_or(&Value::Null),
            _ => &Value::Null,
        };
    }
    cur.clone()
}

fn value_to_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => pretty(other),
    }
}

/// An object as readable lines ("key: value"), nested objects flattened.
pub fn pretty(v: &Value) -> String {
    fn walk(v: &Value, prefix: &str, out: &mut Vec<String>) {
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                    walk(x, &key, out);
                }
            }
            Value::Array(a) => {
                if a.iter().all(|x| !x.is_object() && !x.is_array()) {
                    out.push(format!("{prefix}: {}", a.iter().map(value_to_text).collect::<Vec<_>>().join(", ")));
                } else {
                    for (i, x) in a.iter().enumerate().take(50) {
                        walk(x, &format!("{prefix}[{i}]"), out);
                    }
                }
            }
            other => out.push(if prefix.is_empty() { value_to_text(other) } else { format!("{prefix}: {}", value_to_text(other)) }),
        }
    }
    let mut lines = Vec::new();
    walk(v, "", &mut lines);
    lines.join("\n")
}

/// "$", "$.data", "$.data[*]", "$.a.b[*].c", "$[*]": the matching values.
pub fn select(v: &Value, path: &str) -> Vec<Value> {
    let mut cur = vec![v.clone()];
    let p = path.trim().trim_start_matches('$').trim_start_matches('.');
    if p.is_empty() {
        return explode(cur);
    }
    for part in p.split('.') {
        let (name, star) = match part.strip_suffix("[*]") {
            Some(n) => (n, true),
            None => (part, false),
        };
        let mut next = Vec::new();
        for c in cur {
            let stepped = if name.is_empty() { c } else { get_path(&c, name) };
            if star {
                if let Value::Array(a) = stepped {
                    next.extend(a);
                }
            } else if !stepped.is_null() {
                next.push(stepped);
            }
        }
        cur = next;
    }
    explode(cur)
}

/// A single array result means "the items are its elements".
fn explode(vals: Vec<Value>) -> Vec<Value> {
    if vals.len() == 1 {
        if let Value::Array(a) = &vals[0] {
            return a.clone();
        }
    }
    vals
}

fn safe_id(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).take(120).collect()
}

/// ISO 8601, "YYYY-MM-DD", or epoch seconds/ms → ms.
pub fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<i64>() {
        return Some(if n > 100_000_000_000 { n } else { n * 1000 });
    }
    if s.len() == 10 {
        return crate::integrations::parse_iso_ms(&format!("{s}T00:00:00Z"));
    }
    crate::integrations::parse_iso_ms(s)
}

const README: &str = r#"# Lane connectors

Each `.json` file in this folder is a connector. Lane runs it on a schedule, on this Mac, with your own
key, and turns what comes back into memory: searchable, cited by Ask, and (with "memories": true) into
proper memories with facts and tasks that appear in your briefings.

Files starting with `_` are ignored. Copy an example, drop the `.txt`, edit, save. Lane picks it up
within a minute (or press "Run now" in Settings → Integrations).

Three kinds:
- `http`   fetch a JSON API. Put the key in `headers` as `{{secret}}` and paste the secret in Settings
           (it goes to your Keychain, never into this file).
- `script` run any program you wrote (Python, Node, shell). Print one JSON object per line, or a JSON
           array. Env: LANE_SINCE (ms of the last run), LANE_SECRET.
- `mcp`    call one tool on an MCP server over stdio (command + args + tool + arguments).

Templates: `{{field}}` reads a field of each item (`{{a.b}}` for nested), and these globals:
`{{secret}}`, `{{since}}` (ms), `{{since_iso}}`, `{{since_date}}`, `{{today}}`, `{{yesterday}}`.

Fields per item: `itemId`, `itemTitle`, `itemBody`, `itemTime`, `itemUrl` (templates; sensible defaults
`{{id}}`, `{{title}}`, `{{body}}`, `{{time}}`, `{{url}}`; an empty body becomes "key: value" lines of the
whole item). `items` says where the list is in the response: "$.data[*]", "$.rows", "$".

Nothing here is sent to Lane. The only network calls are the ones you write in these files.
"#;

const EXAMPLE_HTTP: &str = r#"{
  "name": "Website CRM leads",
  "kind": "http",
  "everyMinutes": 60,
  "memories": true,
  "url": "https://crm.example.com/api/leads?updated_since={{since_iso}}",
  "method": "GET",
  "headers": { "Authorization": "Bearer {{secret}}" },
  "items": "$.data[*]",
  "itemId": "{{id}}",
  "itemTitle": "Lead: {{name}} ({{status}})",
  "itemBody": "{{name}} from {{company}} — {{status}}. Value {{value}}. Notes: {{notes}}",
  "itemTime": "{{updated_at}}",
  "itemUrl": "https://crm.example.com/leads/{{id}}"
}
"#;

const EXAMPLE_SCRIPT: &str = r#"{
  "name": "Google Analytics yesterday",
  "kind": "script",
  "everyMinutes": 1440,
  "memories": true,
  "command": "/usr/bin/python3",
  "args": ["/Users/you/lane-connectors/ga_yesterday.py", "{{yesterday}}"],
  "itemId": "ga-{{date}}",
  "itemTitle": "Website traffic {{date}}",
  "itemBody": "Sessions {{sessions}}, users {{users}}, top page {{top_page}}"
}
"#;

const EXAMPLE_MCP: &str = r#"{
  "name": "Sales database (MCP)",
  "kind": "mcp",
  "everyMinutes": 720,
  "memories": true,
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-postgres", "postgresql://localhost/sales"],
  "tool": "query",
  "arguments": { "sql": "SELECT customer, amount, closed_at FROM deals WHERE closed_at >= '{{since_date}}'" }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn select_walks_paths_and_stars() {
        let v = json!({"data": {"rows": [{"id": 1, "a": {"b": "x"}}, {"id": 2, "a": {"b": "y"}}]}});
        assert_eq!(select(&v, "$.data.rows[*]").len(), 2);
        assert_eq!(select(&v, "$.data.rows").len(), 2, "a single array result is exploded");
        assert_eq!(select(&v, "$.data.rows[*].a.b"), vec![json!("x"), json!("y")]);
        assert_eq!(select(&json!([1, 2, 3]), "$").len(), 3);
        assert_eq!(select(&v, "$.missing").len(), 0);
    }

    #[test]
    fn templates_read_fields_and_globals() {
        let item = json!({"name": "Asha", "company": {"name": "Acme"}, "value": 1200, "flag": true});
        let mut g = HashMap::new();
        g.insert("secret".to_string(), "s3".to_string());
        assert_eq!(template("{{name}} at {{company.name}}: {{value}} {{flag}} {{secret}} {{nope}}", &item, &g), "Asha at Acme: 1200 true s3 ");
        assert_eq!(template("no braces", &item, &g), "no braces");
        assert_eq!(template("broken {{x", &item, &g), "broken {{x");
    }

    #[test]
    fn pretty_flattens_objects() {
        let v = json!({"customer": "Acme", "deal": {"amount": 5000, "tags": ["a", "b"]}});
        assert_eq!(pretty(&v), "customer: Acme\ndeal.amount: 5000\ndeal.tags: a, b");
    }

    #[test]
    fn times_parse_in_common_forms() {
        assert_eq!(parse_time("2026-09-18"), Some(1_789_689_600_000));
        assert_eq!(parse_time("1789689600"), Some(1_789_689_600_000));
        assert_eq!(parse_time("1789689600000"), Some(1_789_689_600_000));
        assert_eq!(parse_time(""), None);
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(1_789_689_600_000), "2026-09-18T00:00:00Z");
    }

    #[test]
    fn script_connector_makes_items() {
        let spec = Spec {
            id: "t".into(), name: "Test".into(), kind: "script".into(), every_minutes: 1, memories: false,
            url: String::new(), method: "GET".into(), headers: HashMap::new(), body: None, items: "$".into(),
            command: "/bin/sh".into(), args: vec!["-c".into(), "echo '{\"id\":\"a\",\"title\":\"First\",\"body\":\"hello\",\"time\":\"2026-09-18\"}'; echo '{\"id\":\"b\",\"name\":\"Second\",\"amount\":3}'".into()],
            tool: String::new(), arguments: Value::Null,
            item_id: t_id(), item_title: t_title(), item_body: t_body(), item_time: t_time(), item_url: t_url(),
        };
        let items = run(&spec, None, 0).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].id.as_str(), items[0].title.as_str(), items[0].body.as_str(), items[0].time), ("a", "First", "hello", 1_789_689_600_000));
        assert_eq!(items[1].title, "amount: 3", "no title: first line of the flattened item");
        assert_eq!(items[1].body, "amount: 3\nid: b\nname: Second");
    }

    #[test]
    fn loads_specs_and_reports_broken_ones() {
        let dir = std::env::temp_dir().join(format!("lane-conn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("crm.json"), r#"{"name":"CRM","kind":"http","url":"https://x"}"#).unwrap();
        std::fs::write(dir.join("bad.json"), "{not json").unwrap();
        std::fs::write(dir.join("odd.json"), r#"{"name":"Odd","kind":"ftp"}"#).unwrap();
        std::fs::write(dir.join("_example.json"), r#"{"name":"skip","kind":"http"}"#).unwrap();
        ensure_examples(&dir);
        let specs = load(&dir);
        assert_eq!(specs.len(), 3);
        assert!(matches!(&specs[0], Err((id, _)) if id == "bad"));
        assert!(matches!(&specs[1], Ok(s) if s.id == "crm" && s.every_minutes == 60));
        assert!(matches!(&specs[2], Err((id, e)) if id == "odd" && e.contains("kind")));
        assert!(dir.join("_README.md").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
