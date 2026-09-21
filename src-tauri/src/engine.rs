//! Memory engine: turns closed activities into memory cards with a model
//! that runs on this Mac.
//!
//! The model is served by the bundled llama.cpp runtime (see runtime.rs),
//! started on demand on 127.0.0.1. In development, when the bundled
//! runtime is absent, an Ollama server on its default port is used.
//!
//! Hallucination control: every extracted string must appear literally in
//! the source text or it is dropped and counted. Output is forced into a
//! fixed JSON schema.

use crate::runtime;
use crate::store::{NewMemory, PendingActivity};
use crate::AppState;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const OLLAMA: &str = "http://127.0.0.1:11434";
/// An activity is processed once it has been closed this long.
const SETTLE_MS: i64 = 120_000;
/// Source text handed to the model per activity.
const MAX_INPUT_CHARS: usize = 4_000;
const IDLE_SLEEP: Duration = Duration::from_secs(30);
const BETWEEN_ITEMS: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// The model can be reached and is loaded or loadable.
    pub available: bool,
    pub model: String,
    /// Human-readable state: "ready", "processing", or why it is not available.
    pub detail: String,
    pub busy: bool,
    pub processed: i64,
    pub last_error: Option<String>,
    /// Extracted strings dropped by verification, lifetime of this run.
    pub dropped: i64,
    /// "bundled" or "ollama".
    pub backend: String,
    /// 0-100 while a model file is downloading.
    pub download_percent: Option<u64>,
    pub tokens_per_second: f64,
    pub files_indexed: i64,
    pub files_pending: i64,
    pub files_detail: String,
    pub backup_detail: String,
    pub labels_detail: String,
    pub notion_detail: String,
}

pub const LABELS_ENDPOINT: &str = "https://labels.lane.so/v1/labels";
const LABELS_EVERY_MS: i64 = 6 * 3_600_000;

/// Post unsent labels to Lane. Explicit opt-in; the payload is exactly what
/// Settings shows in the review list.
pub fn send_labels(state: &AppState) -> Result<usize, String> {
    let (token, enabled) = {
        let s = crate::lock(&state.settings);
        (s.tester_token.clone(), s.contribute_labels)
    };
    if !enabled {
        return Err("Contributing labels is off".into());
    }
    if token.trim().is_empty() {
        return Err("Enter your tester code first".into());
    }
    let records = crate::lock(&state.store).unsent_label_records().map_err(|e| e.to_string())?;
    if records.is_empty() {
        return Ok(0);
    }
    let body = json!({"app": "lane", "version": env!("CARGO_PKG_VERSION"), "labels": records.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>()});
    let resp = ureq::post(LABELS_ENDPOINT)
        .set("Authorization", &format!("Bearer {}", token.trim()))
        .timeout(Duration::from_secs(60))
        .send_json(body)
        .map_err(|e| format!("could not reach Lane: {e}"))?;
    if resp.status() >= 300 {
        return Err(format!("Lane refused the upload ({})", resp.status()));
    }
    let ids: Vec<i64> = records.iter().map(|(id, _)| *id).collect();
    let now = crate::capture::now_ms();
    {
        let store = crate::lock(&state.store);
        store.mark_labels_sent(&ids, now).map_err(|e| e.to_string())?;
        let mut s = crate::lock(&state.settings);
        s.labels_last_sent_at = now;
        let _ = store.save_settings(&s);
    }
    log::info!("labels: contributed {}", ids.len());
    set_status(state, |st| st.labels_detail = format!("sent {} labels", ids.len()));
    Ok(ids.len())
}

fn labels_step(state: &AppState) -> bool {
    let (enabled, last) = {
        let s = crate::lock(&state.settings);
        (s.contribute_labels, s.labels_last_sent_at)
    };
    if !enabled || crate::capture::now_ms() - last < LABELS_EVERY_MS {
        return false;
    }
    match send_labels(state) {
        Ok(n) => n > 0,
        Err(e) => {
            set_status(state, |st| st.labels_detail = e);
            // Try again next window, not every tick.
            let mut s = crate::lock(&state.settings);
            s.labels_last_sent_at = crate::capture::now_ms();
            let _ = crate::lock(&state.store).save_settings(&s);
            false
        }
    }
}

const BACKUP_EVERY_MS: i64 = 24 * 3_600_000;

/// Make an encrypted backup now. Called by the user and by the idle loop.
pub fn run_backup(state: &AppState) -> Result<std::path::PathBuf, String> {
    let passphrase = crate::vault::keychain_get(crate::vault::BACKUP_SERVICE).ok_or("No backup passphrase is set")?;
    let folder = {
        let f = crate::lock(&state.settings).backup_folder.clone();
        if f.trim().is_empty() { crate::backup::default_folder() } else { std::path::PathBuf::from(f.trim()) }
    };
    let now = crate::capture::now_ms();
    let stamp = format_time(now).replace(['-', ':'], "").replace(' ', "-");
    set_status(state, |st| st.backup_detail = "backing up".into());
    let result = crate::lock(&state.store).backup(&folder, &passphrase, &stamp);
    match &result {
        Ok(p) => {
            let mut s = crate::lock(&state.settings);
            s.last_backup_at = now;
            let _ = crate::lock(&state.store).save_settings(&s);
            log::info!("backup: wrote {}", p.display());
            set_status(state, |st| st.backup_detail = String::new());
        }
        Err(e) => {
            log::warn!("backup: {e}");
            set_status(state, |st| st.backup_detail = format!("failed: {e}"));
        }
    }
    result
}

/// Once a day, after 06:00 local, write yesterday's briefing so it is ready
/// when the user opens Lane. Cheap when the day was quiet.
fn recap_step(state: &AppState) -> bool {
    let now = crate::capture::now_ms();
    let (today_start, _) = day_bounds(now);
    if now - today_start < 6 * 3_600_000 {
        return false;
    }
    let yesterday = today_start - 1;
    let day = day_of(yesterday);
    let exists = crate::lock(&state.store).recap(&day).map(|r| r.is_some()).unwrap_or(true);
    if exists {
        return false;
    }
    // After a failure, wait half an hour rather than retrying every tick.
    if crate::lock(&LAST_RECAP_FAIL).map_or(false, |t| t.elapsed() < Duration::from_secs(1800)) {
        return false;
    }
    match recap(state, yesterday, false, |_| {}) {
        Ok(_) => true,
        Err(e) => {
            log::warn!("recap: {e}");
            *crate::lock(&LAST_RECAP_FAIL) = Some(std::time::Instant::now());
            false
        }
    }
}

static LAST_RECAP_FAIL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// From the 3rd of each month, one "month in review" for the previous
/// month: older memories roll up into a page instead of piling up.
fn rollup_step(state: &AppState) -> bool {
    let now = crate::capture::now_ms();
    let (today, _) = day_bounds(now);
    let (_, _, d) = ymd(today);
    if d < 3 {
        return false;
    }
    let last_month = month_start(today) - 86_400_000;
    let key = format!("month-{}", &day_of(last_month)[..7]);
    let exists = crate::lock(&state.store).recap(&key).map(|r| r.is_some()).unwrap_or(true);
    if exists {
        return false;
    }
    let had_memories = crate::lock(&state.store).memories_between(month_start(last_month), month_start(today), 1).map(|v| !v.is_empty()).unwrap_or(false);
    if !had_memories {
        // Remember that there was nothing, so this is not retried every tick.
        let _ = crate::lock(&state.store).save_recap(&key, "A quiet month: no memories.", "[]", now);
        return false;
    }
    match recap_span(state, last_month, "month", false, |_| {}) {
        Ok(_) => true,
        Err(e) => {
            log::warn!("rollup: {e}");
            false
        }
    }
}

/// Once a day: raw screen text older than the retention window goes; the
/// memories made from it stay. Meetings, notes and connector items keep
/// their text (it is the memory).
fn retention_step(state: &AppState) -> bool {
    let days = crate::lock(&state.settings).raw_retention_days;
    if days <= 0 {
        return false;
    }
    let now = crate::capture::now_ms();
    {
        let mut last = crate::lock(&LAST_PRUNE);
        if last.map_or(false, |t| now - t < 86_400_000) {
            return false;
        }
        *last = Some(now);
    }
    let mut exempt: Vec<String> = vec!["Meeting".into(), "Note".into(), "Voice note".into()];
    exempt.extend(crate::connectors::load(&connectors_dir(state)).into_iter().flatten().map(|s| s.name));
    if let Ok(images) = crate::lock(&state.store).images_of(None, Some(now - days * 86_400_000)) {
        if !images.is_empty() {
            crate::shots::remove_all(&images);
            log::info!("retention: removed {} pictures older than {days} days", images.len());
        }
    }
    match crate::lock(&state.store).prune_raw(now - days * 86_400_000, &exempt) {
        Ok(n) if n > 0 => {
            log::info!("retention: removed raw text of {n} snapshots older than {days} days");
            true
        }
        Ok(_) => false,
        Err(e) => {
            log::warn!("retention: {e}");
            false
        }
    }
}

static LAST_PRUNE: std::sync::Mutex<Option<i64>> = std::sync::Mutex::new(None);

fn backup_step(state: &AppState) -> bool {
    let (enabled, last) = {
        let s = crate::lock(&state.settings);
        (s.backup_enabled, s.last_backup_at)
    };
    if !enabled || crate::capture::now_ms() - last < BACKUP_EVERY_MS {
        return false;
    }
    let _ = run_backup(state);
    true
}

static DOWNLOAD: runtime::DownloadProgress = runtime::DownloadProgress::new();

/// Which backend serves `model`: a bundled GGUF file or an Ollama tag.
enum Backend {
    Bundled { port: u16 },
    Ollama,
}

/// Make sure a backend for `model` is ready. Downloads and starts the
/// bundled runtime as needed, reporting progress through the status.
fn ensure_backend(state: &AppState, model: &str) -> Result<Backend, String> {
    let is_gguf = model.ends_with(".gguf");
    let runtime_dir = runtime::runtime_dir(state.resource_dir.as_deref());
    if !is_gguf || runtime_dir.is_none() {
        if is_gguf {
            return Err("Bundled model runtime is missing from this build".into());
        }
        return check_model(model).map(|_| Backend::Ollama);
    }
    let runtime_dir = runtime_dir.expect("checked");
    let spec = runtime::spec_for(model).ok_or_else(|| format!("Unknown model file {model}"))?;
    let models_dir = state.db_path.with_file_name("models");
    std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    let model_path = models_dir.join(spec.file);
    if !model_path.is_file() {
        set_status(state, |st| {
            st.available = false;
            st.busy = true;
            st.detail = format!("downloading {} ({:.1} GB)", spec.label, spec.bytes as f64 / 1e9);
            st.download_percent = Some(0);
        });
        let stop = std::sync::atomic::AtomicBool::new(false);
        let result = std::thread::scope(|sc| {
            sc.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    set_status(state, |st| st.download_percent = Some(DOWNLOAD.percent()));
                    std::thread::sleep(Duration::from_secs(1));
                }
            });
            let r = runtime::download(spec, &model_path, &DOWNLOAD);
            stop.store(true, Ordering::Relaxed);
            r
        });
        set_status(state, |st| st.download_percent = None);
        result?;
        log::info!("engine: downloaded {}", spec.file);
    }
    let _starting = crate::lock(&START_LOCK);
    {
        let guard = crate::lock(&runtime::SERVER);
        if let Some(server) = guard.as_ref() {
            if server.model_file == spec.file && runtime::health(server.port) {
                return Ok(Backend::Bundled { port: server.port });
            }
        }
    }
    set_status(state, |st| {
        st.available = false;
        st.busy = true;
        st.detail = format!("loading {}", spec.label);
    });
    let log_path = state.db_path.with_file_name("llama.log");
    let server = runtime::start(&runtime_dir, &model_path, &log_path)?;
    let port = server.port;
    *crate::lock(&runtime::SERVER) = Some(server);
    log::info!("engine: bundled runtime ready on port {port} with {}", spec.file);
    Ok(Backend::Bundled { port })
}

static EMBED_DOWNLOAD: runtime::DownloadProgress = runtime::DownloadProgress::new();
/// Only one thread may start a server at a time: the engine thread and a
/// question or briefing arriving together must share one process, not
/// start two and starve an 8 GB Mac.
static START_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static EMBED_START_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// When the models were last asked for anything. Idle models are unloaded
/// to give the memory back to the user's other apps.
static LAST_MODEL_USE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
const UNLOAD_AFTER: Duration = Duration::from_secs(10 * 60);

fn touch_models() {
    *crate::lock(&LAST_MODEL_USE) = Some(std::time::Instant::now());
}

fn unload_idle_models(state: &AppState) {
    let idle = crate::lock(&LAST_MODEL_USE).map_or(false, |t| t.elapsed() > UNLOAD_AFTER);
    if idle && (crate::lock(&runtime::SERVER).is_some() || crate::lock(&runtime::EMBED_SERVER).is_some()) {
        runtime::shutdown();
        *crate::lock(&LAST_MODEL_USE) = None;
        log::info!("engine: models unloaded after 10 min idle");
        set_status(state, |st| st.detail = "idle, models unloaded".into());
    }
}
static LAST_SCAN: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
const RESCAN_EVERY: Duration = Duration::from_secs(30 * 60);
const FILES_PER_TICK: usize = 3;

fn index_folders(state: &AppState) -> Vec<String> {
    let s = crate::lock(&state.settings);
    if !s.index_files {
        return vec![];
    }
    if s.index_folders.is_empty() { crate::files::default_folders() } else { s.index_folders.clone() }
}

/// (Re)start the folder watcher for the current folders.
pub fn start_file_watcher(state: &AppState) {
    let folders = index_folders(state);
    let watcher = if folders.is_empty() { None } else { crate::files::watch(&folders, state.rescan_files.clone()) };
    *crate::lock(&state.file_watcher) = watcher;
}

/// One idle-time step of file indexing. Returns true if it did work.
fn index_files_step(state: &AppState) -> Result<bool, String> {
    let folders = index_folders(state);
    if folders.is_empty() {
        return Ok(false);
    }
    let due = {
        let last = crate::lock(&LAST_SCAN);
        last.map_or(true, |t| t.elapsed() > RESCAN_EVERY)
    };
    if state.rescan_files.swap(false, Ordering::Relaxed) || due {
        *crate::lock(&LAST_SCAN) = Some(std::time::Instant::now());
        start_file_watcher(state);
        let found = crate::files::scan(&folders);
        let present: std::collections::HashSet<String> = found.iter().map(|c| c.path.clone()).collect();
        let mut queue = Vec::new();
        {
            let store = crate::lock(&state.store);
            let removed = store.remove_missing_files(&present, &folders).map_err(|e| e.to_string())?;
            for c in found {
                if !store.file_is_current(&c.path, c.size, c.mtime).map_err(|e| e.to_string())? {
                    queue.push(c);
                }
            }
            if removed > 0 {
                log::info!("files: removed {removed} deleted files from the index");
            }
        }
        queue.sort_by(|a, b| b.mtime.cmp(&a.mtime)); // newest first
        let n = queue.len();
        *crate::lock(&state.file_queue) = queue.into();
        set_status(state, |st| st.files_pending = n as i64);
        if n > 0 {
            log::info!("files: {n} files to read");
        }
    }
    let batch: Vec<crate::files::Candidate> = {
        let mut q = crate::lock(&state.file_queue);
        (0..FILES_PER_TICK).filter_map(|_| q.pop_front()).collect()
    };
    let mut worked = false;
    for c in batch {
        worked = true;
        set_status(state, |st| st.files_detail = format!("reading {}", std::path::Path::new(&c.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        let (text, took) = crate::files::timed_extract(&c.path);
        let chunks = text.map(|t| crate::files::chunk(&t)).unwrap_or_default();
        if took > Duration::from_secs(20) {
            log::warn!("files: slow importer ({took:?}) for {}", c.path);
        }
        crate::lock(&state.store).upsert_file(&c.path, c.size, c.mtime, &chunks, crate::capture::now_ms()).map_err(|e| e.to_string())?;
    }
    // Vectors for new chunks, a batch at a time.
    let pending = crate::lock(&state.store).file_chunks_without_vectors(16).map_err(|e| e.to_string())?;
    if !pending.is_empty() {
        let port = ensure_embed_backend(state)?;
        let texts: Vec<String> = pending.iter().map(|(_, t)| format!("search_document: {}", t.chars().take(2000).collect::<String>())).collect();
        let vectors = runtime::embed(port, &texts)?;
        let store = crate::lock(&state.store);
        for ((id, _), v) in pending.iter().zip(vectors.iter()) {
            store.store_file_vector(*id, v).map_err(|e| e.to_string())?;
        }
        worked = true;
    }
    let stats = crate::lock(&state.store).file_stats().map_err(|e| e.to_string())?;
    let left = crate::lock(&state.file_queue).len() as i64;
    set_status(state, |st| {
        st.files_indexed = stats.files;
        st.files_pending = left + stats.unembedded / 16;
        st.files_detail = if left > 0 { format!("{left} files to read") } else if stats.unembedded > 0 { "indexing for search".into() } else { "up to date".into() };
    });
    Ok(worked)
}

/// Make sure the embedding server is up. Downloads its model on first use.
fn ensure_embed_backend(state: &AppState) -> Result<u16, String> {
    let runtime_dir = runtime::runtime_dir(state.resource_dir.as_deref()).ok_or("Bundled model runtime is missing from this build")?;
    let spec = &runtime::EMBED_MODEL;
    let models_dir = state.db_path.with_file_name("models");
    std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    let model_path = models_dir.join(spec.file);
    if !model_path.is_file() {
        set_status(state, |st| st.detail = format!("downloading {} ({} MB)", spec.label, spec.bytes / 1_000_000));
        runtime::download(spec, &model_path, &EMBED_DOWNLOAD)?;
        log::info!("engine: downloaded {}", spec.file);
    }
    let _starting = crate::lock(&EMBED_START_LOCK);
    {
        let guard = crate::lock(&runtime::EMBED_SERVER);
        if let Some(server) = guard.as_ref() {
            if runtime::health(server.port) {
                return Ok(server.port);
            }
        }
    }
    let log_path = state.db_path.with_file_name("llama-embed.log");
    let server = runtime::start_embed(&runtime_dir, &model_path, &log_path)?;
    let port = server.port;
    *crate::lock(&runtime::EMBED_SERVER) = Some(server);
    log::info!("engine: embedding server ready on port {port}");
    Ok(port)
}

/// Embed a search question. None when the index is not available, in
/// which case callers fall back to keyword search.
pub fn embed_query(state: &AppState, text: &str) -> Option<Vec<f32>> {
    touch_models();
    let port = ensure_embed_backend(state).ok()?;
    runtime::embed(port, &[format!("search_query: {text}")]).ok()?.into_iter().next()
}

/// Index memories that have no vector yet. Returns how many were done.
fn embed_pending(state: &AppState, limit: u32) -> Result<usize, String> {
    let ids = crate::lock(&state.store).memories_without_vectors(limit).map_err(|e| e.to_string())?;
    if ids.is_empty() {
        return Ok(0);
    }
    let port = ensure_embed_backend(state)?;
    touch_models();
    let texts: Vec<String> = {
        let store = crate::lock(&state.store);
        ids.iter().filter_map(|id| store.embedding_text(*id).ok()).collect()
    };
    let vectors = runtime::embed(port, &texts)?;
    let store = crate::lock(&state.store);
    for (id, vec) in ids.iter().zip(vectors.iter()) {
        store.store_vector(*id, runtime::EMBED_MODEL.file, vec).map_err(|e| e.to_string())?;
    }
    Ok(ids.len())
}

const SYSTEM_PROMPT: &str = "You are a memory assistant running on the user's own computer. You are given text that was on \
their screen during one activity. Decide if it is worth remembering: keep=false for menus, navigation, lists of links or \
titles, ads, empty pages, generic feeds, settings screens; keep=true when there is actual content the user read, wrote or \
worked on. Classify it, give a short specific title about the main content (ignore sidebars, menus and lists of other \
pages), and a 1-2 sentence factual summary of that content. people: real named people in the content. organizations: named \
companies, institutions, agencies. projects: named projects, products, schemes or initiatives the content is actually about \
(not titles of other pages, chats or menu items). dates and numbers: copy EXACTLY as written. tasks: only explicit \
commitments or requests in the content, where someone says they will do something or asks someone to do something (\"I'll \
send the draft by Friday\", \"please review the tender\"); a phrase using the text's own words; NEVER page titles, chat \
names, menu items, list headings, product names, notifications or things merely mentioned. Questions put to the reader (forms, applications, quizzes) are never tasks. Articles, posts, search results and product pages give the reader no tasks: from those return [] unless the user wrote \"I will…\" themselves. Most pages have no tasks: return \
[]. facts: specific values the content states, as subject + attribute + value, e.g. subject \"Vatsalya proposal\", attribute \
\"budget\", value \"₹35 lakh\"; or subject \"tender submission\", attribute \"deadline\", value \"14 October\". The value must be \
copied EXACTLY as written; at most 6, only the ones that matter; most pages have none. For each fact also say owner: \
\"mine\" when it is the user's own (their account, their domains, their proposal, their figure), \"theirs\" when it belongs \
to someone else (another company's price, a listing they browsed), else \"unknown\"; and stance: \"stated\" (a plain \
fact), \"proposed\" (an offer or draft), \"agreed\" (confirmed by both sides), \"asked\" (a question or request). Never \
invent anything. confidence is 0-1 for how sure you are about keep.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "keep": {"type": "boolean"},
            "kind": {"type": "string", "enum": ["email", "chat", "document", "article", "form", "search", "social", "table", "code", "other"]},
            "title": {"type": "string", "maxLength": 120},
            "summary": {"type": "string", "maxLength": 400},
            "people": {"type": "array", "maxItems": 12, "items": {"type": "string", "maxLength": 80}},
            "organizations": {"type": "array", "maxItems": 10, "items": {"type": "string", "maxLength": 80}},
            "dates": {"type": "array", "maxItems": 10, "items": {"type": "string", "maxLength": 40}},
            "numbers": {"type": "array", "maxItems": 12, "items": {"type": "string", "maxLength": 40}},
            "projects": {"type": "array", "maxItems": 6, "items": {"type": "string", "maxLength": 80}},
            "tasks": {"type": "array", "maxItems": 8, "items": {"type": "string", "maxLength": 140}},
            "decisions": {"type": "array", "maxItems": 6, "items": {"type": "string", "maxLength": 140}},
            "facts": {"type": "array", "maxItems": 6, "items": {"type": "object", "properties": {"subject": {"type": "string", "maxLength": 80}, "attribute": {"type": "string", "maxLength": 40}, "value": {"type": "string", "maxLength": 80}, "owner": {"type": "string", "enum": ["mine", "theirs", "unknown"]}, "stance": {"type": "string", "enum": ["stated", "proposed", "agreed", "asked"]}}, "required": ["subject", "attribute", "value", "owner", "stance"]}},
            "confidence": {"type": "number"}
        },
        "required": ["keep", "kind", "title", "summary", "people", "organizations", "dates", "numbers", "projects", "tasks", "decisions", "facts", "confidence"]
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ModelOutput {
    keep: bool,
    kind: String,
    title: String,
    summary: String,
    #[serde(default)]
    people: Vec<String>,
    #[serde(default)]
    organizations: Vec<String>,
    #[serde(default)]
    dates: Vec<String>,
    #[serde(default)]
    numbers: Vec<String>,
    #[serde(default)]
    projects: Vec<String>,
    #[serde(default)]
    tasks: Vec<String>,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    facts: Vec<crate::store::NewFact>,
    #[serde(default)]
    confidence: f64,
}

/// A fact is kept when its value occurs literally in the source and its
/// subject is at least mostly from the source. Attributes are the model's
/// own words (one or two), so they are only length-checked.
pub fn verify_facts(items: Vec<crate::store::NewFact>, source_norm: &str) -> (Vec<crate::store::NewFact>, i64) {
    let mut kept: Vec<crate::store::NewFact> = Vec::new();
    let mut dropped = 0;
    for f in items {
        let value = normalize(&f.value);
        let subject_words: Vec<String> = normalize(&f.subject).split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() > 2).map(String::from).collect();
        let subject_hits = subject_words.iter().filter(|w| source_norm.contains(w.as_str())).count();
        let ok = !value.is_empty()
            && value.chars().count() <= 80
            && source_norm.contains(&value)
            && !subject_words.is_empty()
            && subject_hits * 10 >= subject_words.len() * 7
            && f.attribute.trim().chars().count() <= 40
            && !f.attribute.trim().is_empty();
        let dup = kept.iter().any(|k| crate::store::fact_key(&k.subject, &k.attribute) == crate::store::fact_key(&f.subject, &f.attribute));
        if ok && !dup {
            let owner = if ["mine", "theirs"].contains(&f.owner.as_str()) { f.owner.clone() } else { "unknown".into() };
            let stance = if ["proposed", "agreed", "asked"].contains(&f.stance.as_str()) { f.stance.clone() } else { "stated".into() };
            kept.push(crate::store::NewFact { subject: f.subject.trim().to_string(), attribute: f.attribute.trim().to_lowercase(), value: f.value.trim().to_string(), owner, stance });
        } else if !ok {
            dropped += 1;
        }
    }
    (kept, dropped)
}

/// Tasks are phrased by the model, so exact matching is too strict: keep a
/// task when most of its meaningful words occur in the source.
pub fn verify_tasks(items: Vec<String>, source_norm: &str) -> (Vec<String>, i64) {
    let mut kept: Vec<String> = Vec::new();
    let mut dropped = 0;
    for item in items {
        let words: Vec<String> = normalize(&item)
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() > 3)
            .map(String::from)
            .collect();
        let hits = words.iter().filter(|w| source_norm.contains(w.as_str())).count();
        let ok = !words.is_empty() && hits * 10 >= words.len() * 7 && item.chars().count() >= 8 && !looks_like_form_question(&item);
        if ok && !kept.iter().any(|k| normalize(k) == normalize(&item)) {
            kept.push(item.trim().to_string());
        } else if !ok {
            dropped += 1;
        }
    }
    kept.truncate(8);
    (kept, dropped)
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Application forms and questionnaires read like tasks to a model ("Have
/// you served as a jury member?"); a commitment is something someone will
/// do, never a question put to the reader.
/// One sweep at startup: commitments already stored from forms are closed
/// as dismissed, so the list stops showing questionnaire lines.
pub fn dismiss_form_questions(state: &AppState) {
    let store = crate::lock(&state.store);
    let Ok(tasks) = store.list_tasks("open", 5000) else { return };
    let kinds = store.task_memory_kinds().unwrap_or_default();
    let now = crate::capture::now_ms();
    let mut n = 0;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Oldest first, so a repeated commitment keeps its first appearance.
    let mut ordered = tasks;
    ordered.sort_by_key(|t| t.created_at);
    let mut kept: Vec<String> = Vec::new();
    for t in &ordered {
        let (kind, url) = kinds.get(&t.memory_id).map(|(k, u)| (k.as_str(), u.as_deref())).unwrap_or(("other", None));
        let kind = if ai_chat_source(&t.app_name, url) { "ai-chat" } else { kind };
        let norm: String = t.text.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
        let duplicate = !seen.insert(norm) || kept.iter().any(|k| same_task(k, &t.text));
        let wrong_kind = !kind_carries_commitments(kind) && !first_person_task(&t.text);
        if !(looks_like_form_question(&t.text) || wrong_kind || duplicate) {
            kept.push(t.text.clone());
        }
        if looks_like_form_question(&t.text) || wrong_kind || duplicate {
            if store.set_task_status(t.id, "dismissed", now).is_ok() {
                n += 1;
            }
        }
    }
    if n > 0 {
        log::info!("tasks: dismissed {n} lines that were not commitments (form questions, articles and posts, repeats)");
    }
}

/// Kinds in which someone can actually commit to the user or be asked by
/// them: mail, chat, documents (minutes, notes), and Lane's own notes,
/// meetings and voice notes.
pub fn kind_carries_commitments(kind: &str) -> bool {
    matches!(kind.to_lowercase().as_str(), "email" | "chat" | "document" | "meeting" | "note" | "voice note")
}

/// A chat with an AI is the user instructing a tool, not a commitment to
/// anyone; treated like an article (first-person lines only).
pub fn ai_chat_source(app_name: &str, url: Option<&str>) -> bool {
    const HOSTS: &[&str] = &["chatgpt.com", "chat.openai.com", "claude.ai", "gemini.google.com", "perplexity.ai", "copilot.microsoft.com", "grok.com", "chat.deepseek.com", "poe.com", "chat.mistral.ai", "you.com"];
    const APPS: &[&str] = &["ChatGPT", "Claude", "Perplexity", "Gemini"];
    APPS.iter().any(|a| a.eq_ignore_ascii_case(app_name)) || url.map_or(false, |u| HOSTS.iter().any(|h| u.contains(h)))
}

/// Two phrasings of the same commitment: most meaningful words shared.
pub fn same_task(a: &str, b: &str) -> bool {
    let words = |t: &str| t.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() > 3).map(String::from).collect::<std::collections::HashSet<_>>();
    let (wa, wb) = (words(a), words(b));
    if wa.is_empty() || wb.is_empty() {
        return false;
    }
    let shared = wa.intersection(&wb).count();
    let smaller = wa.len().min(wb.len());
    shared * 10 >= smaller * 7
}

/// "I will…", "I'll…", "Remember to…": a commitment in the user's own voice.
pub fn first_person_task(item: &str) -> bool {
    let l = item.trim().to_lowercase();
    ["i will ", "i'll ", "i’ll ", "i need to ", "i have to ", "i must ", "i should ", "remember to ", "remind me ", "i am going to ", "i'm going to ", "we will ", "we'll ", "we need to "].iter().any(|p| l.starts_with(p))
}

/// An article, a post, a search page or a product page cannot hand the
/// user a task; from those only first-person lines survive. Everything
/// is capped so one long email cannot flood the list.
pub fn tasks_for_kind(kind: &str, tasks: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = if kind_carries_commitments(kind) { tasks } else { tasks.into_iter().filter(|t| first_person_task(t)).collect() };
    out.truncate(5);
    out
}

pub fn looks_like_form_question(item: &str) -> bool {
    let t = item.trim();
    let lower = t.to_lowercase();
    if t.ends_with('?') || t.chars().count() > 140 {
        return true;
    }
    const STARTS: &[&str] = &["why ", "have you", "do you", "did you", "are you", "what ", "which ", "how ", "please share", "please describe", "describe ", "explain ", "kindly ", "if yes", "if no", "in case", "tell us", "mention ", "specify ", "select ", "choose ", "upload ", "enter your", "provide your"];
    STARTS.iter().any(|s| lower.starts_with(s))
}

/// Keep only strings that literally occur in the source. Returns the kept
/// list and how many were dropped.
pub fn verify(items: Vec<String>, source_norm: &str) -> (Vec<String>, i64) {
    let mut kept = Vec::new();
    let mut dropped = 0;
    for item in items {
        let n = normalize(&item);
        if !n.is_empty() && source_norm.contains(&n) {
            if !kept.iter().any(|k: &String| normalize(k) == n) {
                kept.push(item.trim().to_string());
            }
        } else {
            dropped += 1;
        }
    }
    (kept, dropped)
}

/// Close whatever a truncated JSON document left open (a string, arrays,
/// objects) so the part that was generated still parses. The field being
/// written when the budget ran out is dropped, not guessed.
pub fn repair_json(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 8);
    let mut stack: Vec<char> = Vec::new();
    let mut in_str = false;
    let mut escape = false;
    let mut last_str_start = 0usize;
    for (i, c) in raw.char_indices() {
        if in_str {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_str = false;
            }
        } else {
            match c {
                '"' => {
                    in_str = true;
                    last_str_start = i;
                }
                '{' | '[' => stack.push(c),
                '}' | ']' => {
                    stack.pop();
                }
                _ => {}
            }
        }
        out.push(c);
    }
    if in_str {
        // Drop the unfinished string (and the key before it, if it was a value).
        out.truncate(last_str_start);
        let trimmed = out.trim_end().trim_end_matches(':').trim_end().to_string();
        out = if trimmed.ends_with('"') {
            // We were inside a value: also remove its key.
            let key_start = trimmed[..trimmed.len() - 1].rfind('"').unwrap_or(0);
            trimmed[..key_start].trim_end().trim_end_matches(',').to_string()
        } else {
            trimmed
        };
    }
    let mut out = out.trim_end().trim_end_matches(',').to_string();
    // Anything after the last complete value in an object ("key":) is dropped.
    if out.ends_with(':') {
        if let Some(k) = out.rfind('"') {
            if let Some(k0) = out[..k].rfind('"') {
                out.truncate(k0);
                out = out.trim_end().trim_end_matches(',').to_string();
            }
        }
    }
    // An object or array that was opened but never got a complete member is
    // removed rather than closed empty.
    loop {
        let t = out.trim_end().to_string();
        if (t.ends_with('{') || t.ends_with('[')) && stack.len() > 1 {
            stack.pop();
            out = t[..t.len() - 1].trim_end().trim_end_matches(',').to_string();
        } else {
            out = t;
            break;
        }
    }
    for c in stack.iter().rev() {
        out.push(if *c == '{' { '}' } else { ']' });
    }
    out
}

/// Build the memory for one activity from the model's answer.
pub fn build_memory(raw_json: &str, activity: &PendingActivity, text: &str, model: &str) -> Result<NewMemory, String> {
    let out: ModelOutput = match serde_json::from_str(raw_json) {
        Ok(o) => o,
        Err(e) => {
            let repaired = repair_json(raw_json);
            let o: ModelOutput = serde_json::from_str(&repaired).map_err(|_| format!("model returned invalid JSON: {e}"))?;
            if o.kind.is_empty() && o.title.is_empty() && o.summary.is_empty() {
                return Err(format!("model returned invalid JSON: {e}"));
            }
            log::info!("engine: activity {}: output was cut short; kept what was complete", activity.id);
            o
        }
    };
    let source_norm = normalize(&format!("{}\n{}\n{}", activity.window_title, activity.url.clone().unwrap_or_default(), text));
    let (people, d1) = verify(out.people, &source_norm);
    let (organizations, d2) = verify(out.organizations, &source_norm);
    let (dates, d3) = verify(out.dates, &source_norm);
    let (numbers, d4) = verify(out.numbers, &source_norm);
    let (projects, d5) = verify(out.projects, &source_norm);
    let (tasks, d6) = verify_tasks(out.tasks, &source_norm);
    let effective_kind = if ai_chat_source(&activity.app_name, activity.url.as_deref()) { "ai-chat" } else { out.kind.as_str() };
    let tasks = tasks_for_kind(effective_kind, tasks);
    let (decisions, d7) = verify_tasks(out.decisions, &source_norm);
    let (facts, d8) = verify_facts(out.facts, &source_norm);
    let title = out.title.trim().to_string();
    Ok(NewMemory {
        facts,
        kind: out.kind,
        title: if title.is_empty() { activity.window_title.clone() } else { title },
        summary: out.summary.trim().to_string(),
        people,
        organizations,
        dates,
        numbers,
        projects,
        tasks,
        decisions,
        keep: out.keep,
        confidence: out.confidence.clamp(0.0, 1.0),
        dropped: d1 + d2 + d3 + d4 + d5 + d6 + d7 + d8,
        model: model.to_string(),
    })
}

fn prompt_for(activity: &PendingActivity, text: &str) -> String {
    let mins = ((activity.ended_at - activity.started_at) as f64 / 60_000.0).max(0.0);
    format!(
        "App: {}\nWindow: {}\nURL: {}\nTime spent: {:.0} min\n\nTEXT:\n{}",
        activity.app_name,
        activity.window_title,
        activity.url.clone().unwrap_or_default(),
        mins,
        text
    )
}

/// Is the model reachable? Ok(()) or a message for the status line.
fn check_model(model: &str) -> Result<(), String> {
    let resp = ureq::get(&format!("{OLLAMA}/api/tags"))
        .timeout(Duration::from_secs(3))
        .call()
        .map_err(|_| "Local model runtime is not running".to_string())?;
    let tags: Value = resp.into_json().map_err(|e| e.to_string())?;
    let names: Vec<String> = tags["models"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    let wanted = if model.contains(':') { model.to_string() } else { format!("{model}:latest") };
    if names.iter().any(|n| n == &wanted || n == model) {
        Ok(())
    } else {
        Err(format!("Model {model} is not downloaded"))
    }
}

fn run_model(backend: &Backend, model: &str, prompt: &str) -> Result<(String, f64), String> {
    touch_models();
    // A short page cannot need a long answer: a tighter budget finishes sooner.
    let budget = if prompt.chars().count() < 1_800 { 600 } else { 1200 };
    match backend {
        Backend::Bundled { port } => runtime::chat_json(*port, SYSTEM_PROMPT, prompt, schema(), budget),
        Backend::Ollama => run_ollama(model, prompt).map(|c| (c, 0.0)),
    }
}

fn run_ollama(model: &str, prompt: &str) -> Result<String, String> {
    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": prompt}
        ],
        "format": schema(),
        "stream": false,
        "think": false,
        "keep_alive": "10m",
        "options": {"temperature": 0.1, "num_ctx": 4096}
    });
    let resp = ureq::post(&format!("{OLLAMA}/api/chat"))
        .timeout(Duration::from_secs(300))
        .send_json(body)
        .map_err(|e| format!("model call failed: {e}"))?;
    let v: Value = resp.into_json().map_err(|e| e.to_string())?;
    v["message"]["content"].as_str().map(String::from).ok_or_else(|| "empty model response".into())
}


// ── Integrations ─────────────────────────────────────────────────────────

/// Title of the calendar event happening now (started ≤ 15 min ago or
/// starting within 10 min), when the calendar integration is on.
fn current_event_title(state: &AppState, now: i64) -> Option<String> {
    if !crate::lock(&state.settings).calendar_enabled {
        return None;
    }
    let helper = crate::integrations::calendar_helper(state.resource_dir.as_deref())?;
    let text = crate::integrations::run_with_timeout(&helper, "1", Duration::from_secs(5)).ok()?;
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let events: Vec<crate::integrations::Event> = serde_json::from_value(v["events"].clone()).ok()?;
    events
        .into_iter()
        .find(|e| !e.all_day && e.start - 10 * 60_000 <= now && e.end + 15 * 60_000 >= now)
        .map(|e| e.title)
        .filter(|t| !t.trim().is_empty())
}

/// What is worth a note: people dealt with often this week with nothing
/// written down, and calendar meetings from the past week that left no
/// memory within an hour of them.
pub fn memory_gaps(state: &AppState) -> Vec<crate::store::Gap> {
    let now = crate::capture::now_ms();
    let mut gaps = crate::lock(&state.store).memory_gaps(now).unwrap_or_default();
    {
        let store = crate::lock(&state.store);
        if let Ok(overdue) = store.overdue_tasks(now, 50) {
            if overdue.len() >= 3 {
                gaps.insert(0, crate::store::Gap { kind: "overdue".into(), text: format!("{} commitments have waited more than a week. Close them or let them go.", overdue.len()), entity_id: None, question: "What have I promised that is more than a week old, and to whom?".into() });
            }
        }
        if let Ok(quiet) = store.going_quiet(now, 3) {
            for (id, name, at) in quiet {
                if gaps.len() >= 8 {
                    break;
                }
                gaps.push(crate::store::Gap { kind: "quiet".into(), text: format!("{name} used to come up often; nothing since {}.", relative_time(at, now)), entity_id: Some(id), question: format!("What was the last thing with {name}, and is anything open?") });
            }
        }
    }
    if crate::lock(&state.settings).calendar_enabled {
        if let Some(helper) = crate::integrations::calendar_helper(state.resource_dir.as_deref()) {
            // The helper lists upcoming events; past ones come from a negative-hours call.
            if let Ok(events) = crate::integrations::past_events(&helper, 7 * 24) {
                for e in events.into_iter().filter(|e| !e.all_day && e.end < now && e.end > now - 7 * 86_400_000) {
                    let around = crate::lock(&state.store).memories_between(e.start - 3_600_000, e.end + 3_600_000, 3).map(|v| v.len()).unwrap_or(1);
                    if around == 0 && gaps.len() < 6 {
                        gaps.push(crate::store::Gap { kind: "meeting".into(), text: format!("\"{}\" on {} left no memory. Add a note while you still remember it.", e.title, day_of(e.start)), entity_id: None, question: String::new() });
                    }
                }
            }
        }
    }
    gaps
}

static LAST_NOTCH_WATCH: std::sync::Mutex<i64> = std::sync::Mutex::new(0);
/// (last signals refresh ms, last alignment day, last drift nudge ms, last remember-when day)
static SIGNAL_CLOCK: std::sync::Mutex<(i64, String, i64, String)> = std::sync::Mutex::new((0, String::new(), 0, String::new()));

/// Every 30 minutes the day's signals are re-ranked (new memories, new
/// dates); once a day the day is scored against the why, three quiet days
/// in a row earn one nudge, and one old memory on a live thread resurfaces.
fn signals_step(app: &AppHandle, state: &AppState) -> bool {
    let now = crate::capture::now_ms();
    let today = day_of(now);
    let (due_refresh, due_align, due_remember) = {
        let c = crate::lock(&SIGNAL_CLOCK);
        (now - c.0 >= 30 * 60_000, c.1 != today, c.3 != today)
    };
    if !due_refresh && !due_align && !due_remember {
        return false;
    }
    if due_refresh {
        crate::lock(&SIGNAL_CLOCK).0 = now;
        match crate::signals::refresh(state, now) {
            Ok(n) => {
                log::info!("signals: {n} ranked for {today}");
                let _ = app.emit("signals-changed", ());
            }
            Err(e) => log::warn!("signals: {e}"),
        }
    }
    if due_align {
        crate::lock(&SIGNAL_CLOCK).1 = today.clone();
        if let Some((score, memories, near)) = crate::signals::alignment_day(state, now) {
            log::info!("alignment: {today} score {score:.2}, {near} of {memories} near the why");
        }
        let _ = crate::signals::alignment_day(state, now - 86_400_000);
        let days = crate::lock(&state.store).alignment_days(3).unwrap_or_default();
        let quiet = days.len() == 3 && days.iter().all(|(_, _, m, near)| *m >= 3 && *near == 0);
        let nudged_recently = now - crate::lock(&SIGNAL_CLOCK).2 < 7 * 86_400_000;
        if quiet && !nudged_recently {
            crate::lock(&SIGNAL_CLOCK).2 = now;
            notch(app, "reminder", "Three days away from your why", vec!["Nothing captured in three days sat near the why you wrote.".into(), "Open Lane → Explore to see the circle.".into()]);
        }
    }
    if due_remember {
        let (today_start, _) = day_bounds(now);
        if now - today_start >= 11 * 3_600_000 {
            crate::lock(&SIGNAL_CLOCK).3 = today.clone();
            let store = crate::lock(&state.store);
            if let Ok(threads) = store.threads(now - 7 * 86_400_000, 2, 6) {
                let ids: Vec<i64> = threads.iter().map(|(e, _, _)| e.id).collect();
                if let Ok(Some(card)) = store.remember_when(&ids, now - 30 * 86_400_000) {
                    drop(store);
                    notch(app, "help", &format!("Remember when · {}", relative_time(card.started_at, now)), vec![card.title.clone(), card.summary.chars().take(140).collect()]);
                }
            }
        }
    }
    true
}
static LAST_NOTCH_APPS: std::sync::Mutex<(i64, bool)> = std::sync::Mutex::new((0, false));

/// Once a minute: if another notch app is running and the user never chose
/// a position, the tab moves to the top right so the two do not fight over
/// the notch, and says so once.
fn notch_apps_step(app: &AppHandle, state: &AppState) {
    let now = crate::capture::now_ms();
    {
        let mut st = crate::lock(&LAST_NOTCH_APPS);
        if now - st.0 < 60_000 {
            return;
        }
        st.0 = now;
    }
    let apps = crate::running_notch_apps();
    let (pos, chosen) = {
        let s = crate::lock(&state.settings);
        (s.notch_position.clone(), s.notch_position_chosen)
    };
    if apps.is_empty() || chosen || pos != "top-center" {
        return;
    }
    {
        let mut s = crate::lock(&state.settings);
        s.notch_position = "top-right".into();
        let _ = crate::lock(&state.store).save_settings(&s);
    }
    crate::place_notch(app);
    let already = std::mem::replace(&mut crate::lock(&LAST_NOTCH_APPS).1, true);
    if !already {
        log::info!("notch: {} running; tab moved to the top right", apps.join(", "));
        use tauri_plugin_notification::NotificationExt;
        let _ = app.notification().builder().title("Lane moved its tab").body(format!("{} uses the notch, so Lane's tab is at the top right. Change it in Settings → Notch.", apps.join(", "))).show();
    }
}

/// Every 20 s: where the notch is and whether the user can see it, for
/// lane.log. Cheap, and the only way to watch it across Space changes.
fn notch_watch_step(app: &AppHandle) {
    let now = crate::capture::now_ms();
    {
        let mut last = crate::lock(&LAST_NOTCH_WATCH);
        if now - *last < 20_000 {
            return;
        }
        *last = now;
    }
    if let Some(w) = app.get_webview_window("notch") {
        log::info!("notch-watch: {}", crate::window_report(&w));
        if std::env::var("LANE_UNPROTECTED").is_ok() {
            log::info!("windows:\n{}", crate::windows_dump(app));
        }
    }
}

/// Push a card to the notch strip.
pub fn notch(app: &AppHandle, kind: &str, title: &str, lines: Vec<String>) {
    let enabled = app.try_state::<Arc<AppState>>().map_or(true, |s| crate::lock(&s.settings).notch_enabled);
    if !enabled {
        return;
    }
    crate::notch_visible(app, true);
    let _ = app.emit("notch-update", serde_json::json!({"kind": kind, "title": title, "lines": lines, "at": crate::capture::now_ms()}));
}

static LAST_HELP_LEN: std::sync::Mutex<usize> = std::sync::Mutex::new(0);

/// While a meeting is recorded: every new stretch of transcript is read
/// for questions put to the user, commitments, dates and names; memory is
/// searched for what the user knows about them; the notch shows it.
fn meeting_help_step(app: &AppHandle, state: &AppState) -> bool {
    let (recording, live) = {
        let r = crate::lock(&state.recording);
        (r.recording, r.live_text.clone())
    };
    if !recording {
        *crate::lock(&LAST_HELP_LEN) = 0;
        return false;
    }
    let seen = *crate::lock(&LAST_HELP_LEN);
    if live.chars().count() < seen + 220 || state.ask_active.load(Ordering::Relaxed) {
        return false;
    }
    let model = crate::lock(&state.settings).model.clone();
    let Ok(Backend::Bundled { port }) = ensure_backend(state, &model) else { return false };
    *crate::lock(&LAST_HELP_LEN) = live.chars().count();
    let recent: String = live.chars().rev().take(1_400).collect::<Vec<_>>().into_iter().rev().collect();
    let schema = json!({"type": "object", "properties": {
        "questions_for_me": {"type": "array", "maxItems": 3, "items": {"type": "string", "maxLength": 120}},
        "commitments": {"type": "array", "maxItems": 3, "items": {"type": "string", "maxLength": 120}},
        "dates": {"type": "array", "maxItems": 3, "items": {"type": "string", "maxLength": 60}},
        "topics": {"type": "array", "maxItems": 3, "items": {"type": "string", "maxLength": 60}}
    }, "required": ["questions_for_me", "commitments", "dates", "topics"]});
    let system = "You read the latest minute of a live meeting transcript ('You' is the user, 'Others' the other side). Return only what is there: questions the others put to the user, commitments anyone made, dates or deadlines mentioned, and the two or three names or topics the user might need to recall. Short phrases, the transcript's own words, no invention.";
    touch_models();
    let Ok((raw, _)) = runtime::chat_json(port, system, &format!("TRANSCRIPT:\n{recent}"), schema, 220) else { return false };
    let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let list = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).map(String::from).filter(|x| x.chars().count() > 3).collect::<Vec<_>>()).unwrap_or_default();
    let mut lines: Vec<String> = Vec::new();
    for q in list("questions_for_me") {
        lines.push(format!("They asked: {q}"));
    }
    for c in list("commitments") {
        lines.push(format!("Committed: {c}"));
    }
    for d in list("dates") {
        lines.push(format!("Date: {d}"));
    }
    // What the user knows about the names and topics on the table.
    for t in list("topics").into_iter().take(2) {
        let facts = crate::lock(&state.store).search_facts(&t, 2).unwrap_or_default();
        for (f, _, _) in facts {
            lines.push(format!("You know: {} {} {} (as of {})", f.subject, f.attribute, f.value, day_of(f.as_of)));
        }
        if let Ok(cards) = crate::lock(&state.store).search_memories(&t, None, true, 1) {
            if let Some(c) = cards.first() {
                lines.push(format!("Last time on {t}: {} ({})", c.summary.chars().take(110).collect::<String>(), relative_time(c.started_at, crate::capture::now_ms())));
            }
        }
    }
    lines.truncate(7);
    if lines.is_empty() {
        return true;
    }
    log::info!("meeting help: {} lines", lines.len());
    notch(app, "help", "Meeting help", lines);
    true
}

static LAST_TYPED: std::sync::Mutex<Option<(String, std::time::Instant)>> = std::sync::Mutex::new(None);
static LAST_NUDGE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Sentences that appear twice in a text, normalised.
pub fn repeated_sentences(text: &str) -> Vec<String> {
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for sent in text.split(|c| c == '.' || c == '!' || c == '?' || c == '\n') {
        let norm: String = sent.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
        if norm.split_whitespace().count() < 6 {
            continue;
        }
        if seen.contains_key(&norm) {
            if !out.contains(&norm) {
                out.push(sent.trim().to_string());
            }
        } else {
            seen.insert(norm, sent.trim().to_string());
        }
    }
    out
}

/// Opt-in. Every 12 s of typing in a text field: repeated sentences, and
/// figures that disagree with the user's own facts. A quiet nudge in the
/// notch, at most one per 90 s. Nothing typed is stored.
fn thought_check_step(app: &AppHandle, state: &AppState) -> bool {
    let (enabled, settings) = {
        let s = crate::lock(&state.settings);
        (s.thought_check_enabled, s.clone())
    };
    if !enabled || state.ask_active.load(Ordering::Relaxed) {
        return false;
    }
    {
        let last = crate::lock(&LAST_TYPED);
        if last.as_ref().map_or(false, |(_, t)| t.elapsed() < Duration::from_secs(12)) {
            return false;
        }
    }
    if crate::lock(&LAST_NUDGE).map_or(false, |t| t.elapsed() < Duration::from_secs(90)) {
        return false;
    }
    let Some((app_name, text)) = crate::capture::platform::focused_text() else { return false };
    if crate::privacy::is_excluded(&settings, &app_name, None, None) || text.chars().count() < 200 {
        return false;
    }
    let changed = {
        let mut last = crate::lock(&LAST_TYPED);
        let grew = last.as_ref().map_or(true, |(t, _)| (text.chars().count() as i64 - t.chars().count() as i64).abs() >= 80 || !text.ends_with(&t.chars().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect::<String>()));
        *last = Some((text.clone(), std::time::Instant::now()));
        grew
    };
    if !changed {
        return false;
    }
    let mut lines: Vec<String> = repeated_sentences(&text).into_iter().take(2).map(|s| format!("You said this twice: \"{}\"", s.chars().take(90).collect::<String>())).collect();
    // Figures in the text against the user's facts on the same subjects.
    let tail: String = text.chars().rev().take(1_200).collect::<Vec<_>>().into_iter().rev().collect();
    let facts = crate::lock(&state.store).search_facts(&tail, 8).unwrap_or_default();
    if !facts.is_empty() {
        let model = crate::lock(&state.settings).model.clone();
        if let Ok(Backend::Bundled { port }) = ensure_backend(state, &model) {
            let fact_lines: Vec<String> = facts.iter().map(|(f, _, title)| format!("- {} {}: {} (as of {}, from \"{}\")", f.subject, f.attribute, f.value, day_of(f.as_of), title)).collect();
            let schema = json!({"type": "object", "properties": {"contradictions": {"type": "array", "maxItems": 2, "items": {"type": "object", "properties": {"you_wrote": {"type": "string", "maxLength": 100}, "memory_says": {"type": "string", "maxLength": 100}}, "required": ["you_wrote", "memory_says"]}}}, "required": ["contradictions"]});
            let system = "You compare what a person is typing with facts from their own memory and with their own stated principles. Report only clear disagreements about the same thing (a different number, date, name or decision) and only clear crossings of a principle. Quote the typed words and the fact or principle. If nothing disagrees, return an empty list.";
            touch_models();
            if let Ok((raw, _)) = runtime::chat_json(port, system, &format!("TYPING:\n{tail}\n\nFACTS:\n{}{}", fact_lines.join("\n"), { let how = crate::lock(&state.settings).purpose_how.clone(); if how.is_empty() { String::new() } else { format!("\n\nPRINCIPLES (theirs):\n- {}", how.join("\n- ")) } }), schema, 160) {
                if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                    for c in v["contradictions"].as_array().cloned().unwrap_or_default() {
                        if let (Some(a), Some(b)) = (c["you_wrote"].as_str(), c["memory_says"].as_str()) {
                            lines.push(format!("You wrote \"{a}\" but your memory says {b}"));
                        }
                    }
                }
            }
        }
    }
    if lines.is_empty() {
        return false;
    }
    *crate::lock(&LAST_NUDGE) = Some(std::time::Instant::now());
    log::info!("thought check: {} nudges", lines.len());
    notch(app, "help", "While you write", lines);
    true
}

static NOTIFIED_EVENTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static LAST_EVENT_CHECK: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Once a minute: a notification 10–20 minutes before each event, once.
fn meeting_notify_step(app: &AppHandle, state: &AppState) {
    let (enabled, notify) = {
        let s = crate::lock(&state.settings);
        (s.calendar_enabled, s.meeting_notifications)
    };
    if !enabled || !notify {
        return;
    }
    {
        let mut last = crate::lock(&LAST_EVENT_CHECK);
        if last.map_or(false, |t| t.elapsed() < Duration::from_secs(60)) {
            return;
        }
        *last = Some(std::time::Instant::now());
    }
    let Ok(events) = upcoming_events(state, 1) else { return };
    let now = crate::capture::now_ms();
    for e in events.into_iter().filter(|e| !e.all_day) {
        let mins = (e.start - now) / 60_000;
        if !(5..=20).contains(&mins) {
            continue;
        }
        let mut done = crate::lock(&NOTIFIED_EVENTS);
        if done.contains(&e.id) {
            continue;
        }
        done.push(e.id.clone());
        if done.len() > 200 {
            done.remove(0);
        }
        let who = if e.attendees.is_empty() { String::new() } else { format!(" with {}", e.attendees.iter().take(3).cloned().collect::<Vec<_>>().join(", ")) };
        // The prep card: for each person on the invite, what you owe them
        // and the last thing that happened with them.
        let mut lines: Vec<String> = vec![format!("{}{who}.", e.title)];
        {
            let store = crate::lock(&state.store);
            for name in e.attendees.iter().take(3) {
                let first = name.split('@').next().unwrap_or(name).trim();
                if first.is_empty() {
                    continue;
                }
                let Ok(found) = store.list_entities(Some("person"), Some(first), 1) else { continue };
                let Some(person) = found.into_iter().next() else { continue };
                if let Ok(tasks) = store.tasks_for_entity(person.id, 2) {
                    for t in tasks {
                        lines.push(format!("You owe {}: {}", person.name, t.text.chars().take(90).collect::<String>()));
                    }
                }
                if let Ok(cards) = store.entity_memories(person.id, 1) {
                    if let Some(c) = cards.first() {
                        lines.push(format!("Last with {} ({}): {}", person.name, relative_time(c.started_at, now), c.title.chars().take(80).collect::<String>()));
                    }
                }
            }
        }
        if lines.len() == 1 {
            lines.push("Open Lane → Today → Prepare.".into());
        }
        use tauri_plugin_notification::NotificationExt;
        let body = if lines.len() > 1 { lines[1..].iter().take(2).cloned().collect::<Vec<_>>().join(" · ") } else { format!("{}{}.", e.title, who) };
        let _ = app.notification().builder().title(format!("{} in {} min", e.title, mins)).body(body).show();
        log::info!("meeting: notified for \"{}\" ({mins} min)", e.title);
        notch(app, "reminder", &format!("{} in {mins} min", e.title), lines);
        // Prepare the brief now, so Prepare opens instantly when they click it.
        let question = e.question.clone();
        if !question.is_empty() && prepared_answer(&question, now).is_none() {
            if let Ok((answer, sources)) = answer_with(state, &question, &[], false, |_| {}) {
                crate::lock(&PREPARED).push((question, answer, sources, crate::capture::now_ms()));
                log::info!("meeting: prep ready for \"{}\"", e.title);
            }
        }
    }
}

// ── Daily briefing notifications ─────────────────────────────────────────

static LAST_BRIEF_NOTICE: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None); // (day, "morning"|"evening")

/// "07:00" → minutes since local midnight.
fn minutes_of(t: &str) -> Option<i64> {
    let (h, m) = t.split_once(':')?;
    Some(h.parse::<i64>().ok()? * 60 + m.parse::<i64>().ok()?)
}

/// At the morning time: yesterday's briefing (written if needed) as a
/// notification. At the evening time: today's day so far, written fresh.
/// Each fires once per day.
fn briefing_notify_step(app: &AppHandle, state: &AppState) -> bool {
    let (morning, evening) = {
        let s = crate::lock(&state.settings);
        (s.morning_brief_at.clone(), s.evening_brief_at.clone())
    };
    let now = crate::capture::now_ms();
    let (today_start, _) = day_bounds(now);
    let minute = (now - today_start) / 60_000;
    let today = day_of(now);
    let due = |t: &str| minutes_of(t).map_or(false, |m| minute >= m && minute < m + 90);
    let already = |which: &str| crate::lock(&LAST_BRIEF_NOTICE).as_ref().map_or(false, |(d, w)| d == &today && w == which);
    // A notice for a slot is remembered as the last one; morning then evening.
    let slot = if !morning.is_empty() && due(&morning) && !already("morning") && !already("evening") {
        Some("morning")
    } else if !evening.is_empty() && due(&evening) && !already("evening") {
        Some("evening")
    } else {
        None
    };
    let Some(which) = slot else { return false };
    *crate::lock(&LAST_BRIEF_NOTICE) = Some((today.clone(), which.into()));
    let (title, text) = if which == "morning" {
        let yesterday = today_start - 1;
        match recap(state, yesterday, false, |_| {}) {
            Ok((t, _)) => ("Your morning brief".to_string(), t),
            Err(e) => {
                log::warn!("brief: {e}");
                return false;
            }
        }
    } else {
        match recap(state, now, true, |_| {}) {
            Ok((t, _)) => {
                let moved = crate::lock(&state.store).signals_for(&day_of(now)).map(|v| {
                    let chosen: Vec<_> = v.iter().filter(|s| s.rank <= 3 && s.state != "noise").collect();
                    let done = chosen.iter().filter(|s| s.state == "done").count();
                    if chosen.is_empty() { String::new() } else { format!("What moved: {done} of {} things done.", chosen.len()) }
                }).unwrap_or_default();
                ("Your day so far".to_string(), if moved.is_empty() { t } else { format!("{moved}\n{t}") })
            }
            Err(e) => {
                log::warn!("brief: {e}");
                return false;
            }
        }
    };
    let first: String = text.split(|c| c == '.' || c == '\n').next().unwrap_or("").trim().chars().take(160).collect();
    let mut lines: Vec<String> = text.split('\n').filter(|l| !l.trim().is_empty()).take(2).map(|l| l.chars().take(160).collect()).collect();
    let mut nudge = String::new();
    if which == "morning" {
        // The three things lead the morning.
        let _ = crate::signals::refresh(state, now);
        if let Ok(sigs) = crate::lock(&state.store).signals_for(&day_of(now)) {
            let top: Vec<String> = sigs.iter().filter(|s| s.state == "open" || s.state == "pinned").take(3).enumerate().map(|(i, s)| format!("{}. {}{}", i + 1, s.title.chars().take(80).collect::<String>(), if s.reason.is_empty() { String::new() } else { format!(" ({})", s.reason) })).collect();
            if !top.is_empty() {
                lines.insert(0, format!("Three things: {}", top.join("  ")));
            }
        }
    }
    if which == "morning" {
        // The two nudges nobody else gives: promises going stale, people going quiet.
        let store = crate::lock(&state.store);
        if let Ok(overdue) = store.overdue_tasks(now, 3) {
            if !overdue.is_empty() {
                let l = format!("Past a week, still open: {}", overdue.iter().map(|t| t.text.chars().take(60).collect::<String>()).collect::<Vec<_>>().join(" · "));
                nudge = l.clone();
                lines.push(l);
            }
        }
        if let Ok(quiet) = store.going_quiet(now, 3) {
            if !quiet.is_empty() {
                lines.push(format!("Gone quiet: {}", quiet.iter().map(|(_, n, at)| format!("{n} ({})", relative_time(*at, now))).collect::<Vec<_>>().join(", ")));
            }
        }
    }
    use tauri_plugin_notification::NotificationExt;
    let body = if nudge.is_empty() { format!("{first}. Open Lane → Today for the rest.") } else { format!("{first}. {nudge}") };
    let _ = app.notification().builder().title(title.clone()).body(body).show();
    notch(app, "brief", &title, lines);
    log::info!("brief: {which} notification sent");
    true
}

pub fn upcoming_events(state: &AppState, hours: u32) -> Result<Vec<crate::integrations::Event>, String> {
    if !crate::lock(&state.settings).calendar_enabled {
        return Ok(vec![]);
    }
    let helper = crate::integrations::calendar_helper(state.resource_dir.as_deref()).ok_or("calendar helper missing from this build")?;
    crate::integrations::upcoming_events(&helper, hours)
}

const NOTION_PREFIX: &str = "notion://";
const NOTION_EVERY_MS: i64 = 30 * 60_000;
const NOTION_PAGES_PER_TICK: usize = 8;
static NOTION_QUEUE: std::sync::Mutex<std::collections::VecDeque<crate::integrations::NotionPage>> = std::sync::Mutex::new(std::collections::VecDeque::new());

fn notion_token() -> Option<String> {
    crate::vault::keychain_get(crate::integrations::NOTION_SERVICE).filter(|t| !t.is_empty())
}

/// One idle-time step: list changed Notion pages (every 30 min or on
/// request), then import a few per tick into the file index. Returns true
/// if it did work.
fn notion_step(state: &AppState, force: bool) -> Result<bool, String> {
    let (enabled, last) = {
        let s = crate::lock(&state.settings);
        (s.notion_enabled, s.notion_last_sync_at)
    };
    if !enabled {
        return Ok(false);
    }
    let Some(token) = notion_token() else {
        set_status(state, |st| st.notion_detail = "no token".into());
        return Ok(false);
    };
    let now = crate::capture::now_ms();
    let mut worked = false;
    if force || now - last >= NOTION_EVERY_MS {
        set_status(state, |st| st.notion_detail = "listing pages".into());
        let pages = crate::integrations::notion_pages(&token)?;
        let present: std::collections::HashSet<String> = pages.iter().map(|p| format!("{NOTION_PREFIX}{}", p.id)).collect();
        let mut queue = Vec::new();
        {
            let store = crate::lock(&state.store);
            let removed = store.remove_files_with_prefix(NOTION_PREFIX, &present).map_err(|e| e.to_string())?;
            if removed > 0 {
                log::info!("notion: removed {removed} pages no longer shared");
            }
            for p in pages {
                if !store.file_is_current(&format!("{NOTION_PREFIX}{}", p.id), 0, p.edited_ms).map_err(|e| e.to_string())? {
                    queue.push(p);
                }
            }
        }
        log::info!("notion: {} pages to import", queue.len());
        *crate::lock(&NOTION_QUEUE) = queue.into();
        let mut s = crate::lock(&state.settings);
        s.notion_last_sync_at = now;
        let _ = crate::lock(&state.store).save_settings(&s);
        worked = true;
    }
    let batch: Vec<crate::integrations::NotionPage> = {
        let mut q = crate::lock(&NOTION_QUEUE);
        (0..NOTION_PAGES_PER_TICK).filter_map(|_| q.pop_front()).collect()
    };
    for p in batch {
        worked = true;
        set_status(state, |st| st.notion_detail = format!("importing {}", p.title));
        let text = match crate::integrations::notion_page_text(&token, &p.id) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("notion: {}: {e}", p.title);
                continue;
            }
        };
        let chunks = crate::files::chunk(&text);
        crate::lock(&state.store)
            .upsert_file_named(&format!("{NOTION_PREFIX}{}", p.id), &p.title, "notion", 0, p.edited_ms, &chunks, now)
            .map_err(|e| e.to_string())?;
    }
    let left = crate::lock(&NOTION_QUEUE).len();
    let count = crate::lock(&state.store).count_files_with_prefix(NOTION_PREFIX).unwrap_or(0);
    set_status(state, |st| st.notion_detail = if left > 0 { format!("{left} pages to import") } else { format!("{count} pages") });
    Ok(worked)
}

/// Import Notion now (from the UI); the engine thread picks up the queue.
// ── Connectors ───────────────────────────────────────────────────────────

const CONNECTOR_PREFIX: &str = "connector://";
static CONNECTORS_CHECKED: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
static CONNECTOR_FORCE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn connectors_dir(state: &AppState) -> std::path::PathBuf {
    crate::connectors::dir(state.db_path.parent().unwrap_or(std::path::Path::new(".")))
}

/// Run every connector that is due (checked once a minute), or the ones
/// asked for with `run_connector_now`. Returns true if any ran.
fn connectors_step(state: &AppState) -> bool {
    let forced: Vec<String> = std::mem::take(&mut *crate::lock(&CONNECTOR_FORCE));
    if forced.is_empty() {
        let mut checked = crate::lock(&CONNECTORS_CHECKED);
        if checked.map_or(false, |t| t.elapsed() < Duration::from_secs(60)) {
            return false;
        }
        *checked = Some(std::time::Instant::now());
    }
    let now = crate::capture::now_ms();
    let mut ran = false;
    for spec in crate::connectors::load(&connectors_dir(state)).into_iter().flatten() {
        let (last_run, _, _) = crate::lock(&state.store).connector_run(&spec.id).unwrap_or((0, 0, String::new()));
        let due = now - last_run >= (spec.every_minutes.max(1) as i64) * 60_000;
        if !forced.contains(&spec.id) && !due {
            continue;
        }
        ran = true;
        let secret = crate::vault::keychain_get(&crate::connectors::secret_service(&spec.id));
        match crate::connectors::run(&spec, secret.as_deref(), last_run) {
            Ok(items) => {
                let n = items.len();
                match ingest_items(state, &spec, items, now) {
                    Ok(new) => {
                        log::info!("connector {}: {n} items, {new} new", spec.id);
                        let _ = crate::lock(&state.store).set_connector_run(&spec.id, now, n as i64, "");
                    }
                    Err(e) => {
                        log::warn!("connector {}: {e}", spec.id);
                        let _ = crate::lock(&state.store).set_connector_run(&spec.id, now, n as i64, &e);
                    }
                }
            }
            Err(e) => {
                log::warn!("connector {}: {e}", spec.id);
                let _ = crate::lock(&state.store).set_connector_run(&spec.id, now, 0, &e);
            }
        }
    }
    ran
}

/// Items into the file index; new or changed ones also become activities
/// when the connector asks for memories. Returns how many were new.
fn ingest_items(state: &AppState, spec: &crate::connectors::Spec, items: Vec<crate::connectors::Item>, now: i64) -> Result<usize, String> {
    let store = crate::lock(&state.store);
    let prefix = format!("{CONNECTOR_PREFIX}{}/", spec.id);
    let present: std::collections::HashSet<String> = items.iter().map(|i| format!("{prefix}{}", i.id)).collect();
    // Connectors that return "everything" (no since) prune what vanished;
    // incremental ones must not.
    if !spec.url.contains("{{since") && !spec.args.iter().any(|a| a.contains("{{since")) && spec.kind != "mcp" {
        store.remove_files_with_prefix(&prefix, &present).map_err(|e| e.to_string())?;
    }
    let mut new = 0;
    for it in items {
        let path = format!("{prefix}{}", it.id);
        let text = format!("{}
{}", it.title, it.body);
        let hash = crate::store::text_hash(&text);
        if store.file_has_hash(&path, &hash).map_err(|e| e.to_string())? {
            continue;
        }
        new += 1;
        let chunks = crate::files::chunk(&text);
        let time = if it.time > 0 { it.time } else { now };
        store.upsert_file_named(&path, &it.title, &spec.name, text.len() as u64, time, &chunks, now).map_err(|e| e.to_string())?;
        store.set_file_url(&path, &it.url).map_err(|e| e.to_string())?;
        if spec.memories {
            let url = if it.url.is_empty() { None } else { Some(it.url.as_str()) };
            store.create_source_activity(&spec.name, &it.title, url, &text, time).map_err(|e| e.to_string())?;
        }
    }
    if new > 0 && spec.memories {
        state.engine_wake.store(true, Ordering::Relaxed);
    }
    Ok(new)
}

/// Apple Mail is a built-in script connector: the spec file is written
/// (or removed) to match the setting, pointing at the bundled script.
pub fn apply_mail_setting(state: &AppState) {
    let (enabled, memories) = {
        let s = crate::lock(&state.settings);
        (s.mail_enabled, s.mail_memories)
    };
    let dir = connectors_dir(state);
    let path = dir.join("apple-mail.json");
    if !enabled {
        if path.exists() {
            let _ = std::fs::remove_file(&path);
            let empty = std::collections::HashSet::new();
            let removed = crate::lock(&state.store).remove_files_with_prefix("connector://apple-mail/", &empty).unwrap_or(0);
            log::info!("mail: turned off, removed {removed} indexed messages");
        }
        return;
    }
    let script = state.resource_dir.as_ref().map(|r| r.join("scripts").join("mail.js")).filter(|p| p.is_file()).unwrap_or_else(|| std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/mail.js")));
    let spec = serde_json::json!({
        "name": "Apple Mail",
        "kind": "script",
        "everyMinutes": 30,
        "memories": memories,
        "command": "/usr/bin/osascript",
        "args": ["-l", "JavaScript", script.display().to_string(), "{{since}}"],
        "itemId": "{{id}}", "itemTitle": "{{title}}", "itemBody": "{{body}}", "itemTime": "{{time}}", "itemUrl": "{{url}}"
    });
    let _ = std::fs::create_dir_all(&dir);
    if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&spec).unwrap_or_default()) {
        log::warn!("mail: could not write connector: {e}");
        return;
    }
    run_connector_now(state, "apple-mail");
}

static LAST_CONTACTS_CHECK: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Daily: first names and nicknames from Contacts become aliases of the
/// people Lane already knows by full name, when unambiguous.
fn contacts_step(state: &AppState) -> bool {
    let (enabled, last) = {
        let s = crate::lock(&state.settings);
        (s.contacts_enabled, s.contacts_last_sync_at)
    };
    let now = crate::capture::now_ms();
    if !enabled || now - last < 86_400_000 {
        return false;
    }
    {
        let mut checked = crate::lock(&LAST_CONTACTS_CHECK);
        if checked.map_or(false, |t| t.elapsed() < Duration::from_secs(300)) {
            return false;
        }
        *checked = Some(std::time::Instant::now());
    }
    let mut candidates = Vec::new();
    if let Some(r) = &state.resource_dir {
        candidates.push(r.join("audio").join("lane-contacts"));
    }
    candidates.push(std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/audio/lane-contacts")));
    let Some(helper) = candidates.into_iter().find(|p| p.is_file()) else { return false };
    let text = match crate::integrations::run_with_timeout(&helper, "", Duration::from_secs(150)) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("contacts: {e}");
            return false;
        }
    };
    let v: serde_json::Value = match serde_json::from_str(text.trim()) {
        Ok(v) => v,
        Err(_) => return false,
    };
    if let Some(e) = v["error"].as_str() {
        log::warn!("contacts: {e}");
        return false;
    }
    let contacts: Vec<serde_json::Value> = v["contacts"].as_array().cloned().unwrap_or_default();
    // A first name or nickname is an alias only when it names one contact.
    let mut by_short: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for c in &contacts {
        let given = c["given"].as_str().unwrap_or("").trim();
        let family = c["family"].as_str().unwrap_or("").trim();
        let nick = c["nickname"].as_str().unwrap_or("").trim();
        if given.is_empty() || family.is_empty() {
            continue;
        }
        let full = format!("{given} {family}");
        by_short.entry(given.to_lowercase()).or_default().push(full.clone());
        if !nick.is_empty() {
            by_short.entry(nick.to_lowercase()).or_default().push(full);
        }
    }
    let mut added = 0;
    {
        let store = crate::lock(&state.store);
        for (short, fulls) in &by_short {
            if fulls.len() != 1 || short.chars().count() < 2 {
                continue;
            }
            if let Ok(Some(id)) = store.person_id_by_name(&fulls[0]) {
                if !store.alias_exists(short).unwrap_or(true) {
                    if store.set_alias(id, short).is_ok() {
                        added += 1;
                    }
                }
            }
        }
    }
    log::info!("contacts: {} contacts read, {added} aliases added", contacts.len());
    let mut s = crate::lock(&state.settings);
    s.contacts_last_sync_at = now;
    let _ = crate::lock(&state.store).save_settings(&s);
    true
}

pub fn run_connector_now(state: &AppState, id: &str) {
    crate::lock(&CONNECTOR_FORCE).push(id.to_string());
    state.engine_wake.store(true, Ordering::Relaxed);
}

pub fn connector_reports(state: &AppState) -> Vec<crate::connectors::RunReport> {
    let dir = connectors_dir(state);
    let mut out = Vec::new();
    for entry in crate::connectors::load(&dir) {
        match entry {
            Ok(spec) => {
                let (last_run, items, error) = crate::lock(&state.store).connector_run(&spec.id).unwrap_or((0, 0, String::new()));
                out.push(crate::connectors::RunReport { file: dir.join(format!("{}.json", spec.id)).display().to_string(), id: spec.id, name: spec.name, kind: spec.kind, every_minutes: spec.every_minutes, memories: spec.memories, last_run, items, error });
            }
            Err((id, e)) => out.push(crate::connectors::RunReport { file: dir.join(format!("{id}.json")).display().to_string(), id: id.clone(), name: id, kind: "invalid".into(), error: e, ..Default::default() }),
        }
    }
    out
}

pub fn sync_notion_now(state: &AppState) -> Result<(), String> {
    notion_step(state, true).map(|_| ())?;
    state.engine_wake.store(true, Ordering::Relaxed);
    Ok(())
}

pub fn disconnect_notion(state: &AppState) -> Result<(), String> {
    crate::vault::keychain_delete(crate::integrations::NOTION_SERVICE);
    crate::lock(&NOTION_QUEUE).clear();
    let empty = std::collections::HashSet::new();
    let removed = crate::lock(&state.store).remove_files_with_prefix(NOTION_PREFIX, &empty).map_err(|e| e.to_string())?;
    log::info!("notion: disconnected, removed {removed} pages");
    let mut s = crate::lock(&state.settings);
    s.notion_enabled = false;
    s.notion_last_sync_at = 0;
    let _ = crate::lock(&state.store).save_settings(&s);
    set_status(state, |st| st.notion_detail = String::new());
    Ok(())
}

/// The parent page id from a pasted id or URL.
pub fn notion_page_id(s: &str) -> Option<String> {
    // Notion URLs end in the 32-hex id, sometimes after a slug: "…/Title-<id>".
    let tail = s.trim().rsplit('/').next()?.split(['?', '#']).next()?;
    let hex: String = tail.chars().rev().filter(|c| c.is_ascii_hexdigit()).take(32).collect::<Vec<_>>().into_iter().rev().collect();
    if hex.len() != 32 || tail.chars().filter(|c| c.is_ascii_hexdigit()).count() < 32 {
        return None;
    }
    Some(format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..]))
}

pub fn export_to_notion(state: &AppState, title: &str, text: &str) -> Result<String, String> {
    let token = notion_token().ok_or("Notion is not connected")?;
    let parent = crate::lock(&state.settings).notion_parent_page.clone();
    let parent = notion_page_id(&parent).ok_or("Set the Notion page Lane should write under (Settings → Integrations)")?;
    crate::integrations::notion_create_page(&token, &parent, title, text)
}

/// A compiled profile of a person, organisation or project: everything Lane
/// knows, written once and cached until the entity is mentioned again.
pub fn entity_profile(state: &AppState, id: i64, force: bool, mut on_token: impl FnMut(&str)) -> Result<(String, Vec<AskSource>), String> {
    let entity = crate::lock(&state.store).entity(id).map_err(|e| e.to_string())?;
    if !force {
        if let Ok(Some(p)) = crate::lock(&state.store).entity_profile(id) {
            on_token(&p);
            return Ok((p, vec![]));
        }
    }
    let what = match entity.kind.as_str() {
        "person" => "Who is",
        "org" => "What is",
        _ => "What is the state of",
    };
    let question = format!(
        "{what} {}? Write a profile: their role and how I know them, what we have worked on or discussed, any numbers, dates and decisions, and what is still open. Use only what the sources say.",
        entity.name
    );
    let (text, sources) = answer_with(state, &question, &[], false, on_token)?;
    let _ = crate::lock(&state.store).set_entity_profile(id, &text, crate::capture::now_ms());
    Ok((text, sources))
}

pub fn spawn(app: AppHandle, state: Arc<AppState>) {
    dismiss_form_questions(&state);
    // Checked once a minute inside the loop below.
    let mut last_licence: i64 = 0;
    std::thread::Builder::new()
        .name("memory-engine".into())
        .spawn(move || loop {
            // "Pause capture" stops recording, not the making of memories
            // from what was already recorded.
            let model = crate::lock(&state.settings).model.clone();

            // Don't load a 2.5 GB model just to say "ready": wait for work.
            let nothing_pending = !crate::lock(&state.store).memory_counts().map(|c| c.pending > 0).unwrap_or(false);
            let model_downloaded = model.ends_with(".gguf") && state.db_path.with_file_name("models").join(&model).is_file();
            if nothing_pending && crate::lock(&runtime::SERVER).is_none() && model_downloaded {
                unload_idle_models(&state);
                set_status(&state, |st| {
                    st.available = true;
                    st.busy = false;
                    st.model = model.clone();
                    st.detail = "idle".into();
                });
                // Still keep the search index and files current.
                if let Ok(n) = embed_pending(&state, 16) { if n > 0 { continue; } }
                meeting_notify_step(&app, &state);
                if briefing_notify_step(&app, &state) { continue; }
                if meeting_help_step(&app, &state) { continue; }
                meeting_screen_step(&state);
                call_detect_step(&app, &state);
                silence_step(&app, &state);
                if thought_check_step(&app, &state) { continue; }
                if let Ok(true) = index_files_step(&state) { continue; }
                match notion_step(&state, false) {
                    Ok(true) => continue,
                    Err(e) => { log::warn!("notion: {e}"); set_status(&state, |st| st.notion_detail = e); }
                    _ => {}
                }
                if connectors_step(&state) { continue; }
                if backup_step(&state) { continue; }
                sleep_or_wake(&state, IDLE_SLEEP);
                continue;
            }
            let backend = match ensure_backend(&state, &model) {
                Ok(b) => b,
                Err(msg) => {
                    let changed = {
                        let e = crate::lock(&state.engine);
                        e.available || e.detail != msg
                    };
                    if changed {
                        log::warn!("engine: {msg}");
                    }
                    set_status(&state, |st| {
                        st.available = false;
                        st.busy = false;
                        st.model = model.clone();
                        st.detail = msg;
                    });
                    sleep_or_wake(&state, IDLE_SLEEP);
                    continue;
                }
            };
            let backend_name = if matches!(backend, Backend::Bundled { .. }) { "bundled" } else { "ollama" };
            touch_models();

            let now = crate::capture::now_ms();
            let has_pending = crate::lock(&state.store).memory_counts().map(|c| c.pending > 0).unwrap_or(false);
            if !has_pending {
                unload_idle_models(&state);
            }
            // Due connectors run between memories too; the step is cheap
            // (its own once-a-minute check) and a remake must not starve them.
            connectors_step(&state);
            notch_watch_step(&app);
            notch_apps_step(&app, &state);
            signals_step(&app, &state);
            meeting_notify_step(&app, &state);
            briefing_notify_step(&app, &state);
            meeting_help_step(&app, &state);
            meeting_screen_step(&state);
            call_detect_step(&app, &state);
            silence_step(&app, &state);
            thought_check_step(&app, &state);
            // Once a minute, ask the Keychain where we stand. Nothing leaves
            // the Mac; this is a date and a signature check.
            if now - last_licence > 60_000 {
                last_licence = now;
                let st = crate::licence::status();
                let was = state.blocked.swap(st.blocked, Ordering::Relaxed);
                if was != st.blocked {
                    log::info!("licence: {} ({} days left)", st.state, st.days_left);
                    if st.blocked {
                        notch(&app, "help", "Trial ended", vec!["Lane has stopped remembering. Your memories are safe; enter a key in Settings to carry on.".into()]);
                    }
                }
            }
            if state.blocked.load(Ordering::Relaxed) {
                sleep_or_wake(&state, IDLE_SLEEP);
                continue;
            }
            let next = match crate::lock(&state.store).next_pending_activity(now - SETTLE_MS) {
                Ok(n) => n,
                Err(e) => {
                    log::error!("engine: pending query failed: {e}");
                    None
                }
            };
            let Some(activity) = next else {
                // Nothing to make: keep the search index complete instead.
                match embed_pending(&state, 16) {
                    Ok(n) if n > 0 => {
                        log::info!("engine: indexed {n} memories for search");
                        sleep_or_wake(&state, BETWEEN_ITEMS);
                        continue;
                    }
                    Err(e) => log::warn!("engine: indexing: {e}"),
                    _ => {}
                }
                match index_files_step(&state) {
                    Ok(true) => {
                        sleep_or_wake(&state, Duration::from_millis(300));
                        continue;
                    }
                    Err(e) => log::warn!("files: {e}"),
                    _ => {}
                }
                match notion_step(&state, false) {
                    Ok(true) => {
                        sleep_or_wake(&state, Duration::from_millis(300));
                        continue;
                    }
                    Err(e) => {
                        log::warn!("notion: {e}");
                        set_status(&state, |st| st.notion_detail = e);
                    }
                    _ => {}
                }
                if connectors_step(&state) {
                    continue;
                }
                if contacts_step(&state) {
                    continue;
                }
                if backup_step(&state) {
                    continue;
                }
                if retention_step(&state) {
                    continue;
                }
                if recap_step(&state) {
                    continue;
                }
                if rollup_step(&state) {
                    continue;
                }
                if labels_step(&state) {
                    continue;
                }
                set_status(&state, |st| {
                    st.available = true;
                    st.busy = false;
                    st.model = model.clone();
                    st.backend = backend_name.into();
                    st.detail = "ready".into();
                });
                sleep_or_wake(&state, IDLE_SLEEP);
                continue;
            };
            if state.ask_active.load(Ordering::Relaxed) {
                // A question is being answered; give it the model.
                sleep_or_wake(&state, BETWEEN_ITEMS);
                continue;
            }

            set_status(&state, |st| {
                st.available = true;
                st.busy = true;
                st.model = model.clone();
                st.backend = backend_name.into();
                st.detail = format!("processing: {}", activity.window_title);
            });

            let text = crate::lock(&state.store).activity_text(activity.id, MAX_INPUT_CHARS).unwrap_or_default();
            let result = run_model(&backend, &model, &prompt_for(&activity, &text))
                .and_then(|(raw, tps)| build_memory(&raw, &activity, &text, &model).map(|m| (m, tps)));

            match result {
                Ok((memory, tps)) => {
                    log::info!("engine: memory for activity {} ({} dropped, {tps:.1} tok/s)", activity.id, memory.dropped);
                    let dropped = memory.dropped;
                    if let Err(e) = crate::lock(&state.store).insert_memory(activity.id, &memory, crate::capture::now_ms()) {
                        log::error!("insert memory: {e}");
                    }
                    set_status(&state, |st| {
                        st.processed += 1;
                        st.dropped += dropped;
                        st.last_error = None;
                        if tps > 0.0 {
                            st.tokens_per_second = tps;
                        }
                    });
                    let _ = app.emit("memories-changed", ());
                    if let Err(e) = embed_pending(&state, 4) {
                        log::warn!("engine: indexing: {e}");
                    }
                }
                Err(e) => {
                    let transient = e.contains("status code 5") || e.contains("Network Error") || e.contains("Unexpected EOF") || e.contains("timed out");
                    log::warn!("memory engine: activity {}: {e}{}", activity.id, if transient { " (runtime restarts)" } else { "" });
                    if transient {
                        // The server, not the text, is the problem: leave the
                        // activity pending, restart the runtime, try again.
                        let _ = crate::lock(&state.store).set_memory_status(activity.id, "pending");
                        runtime::shutdown();
                        // Not the user's problem to read: no last_error, just a status.
                        set_status(&state, |st| { st.available = false; st.detail = "restarting the model runtime".into(); });
                        sleep_or_wake(&state, Duration::from_secs(5));
                        continue;
                    }
                    let _ = crate::lock(&state.store).set_memory_status(activity.id, "failed");
                    set_status(&state, |st| st.last_error = Some(e));
                }
            }
            sleep_or_wake(&state, BETWEEN_ITEMS);
        })
        .expect("spawn memory engine");
}

fn set_status(state: &AppState, f: impl FnOnce(&mut EngineStatus)) {
    f(&mut crate::lock(&state.engine));
}

/// Sleep, but return early when someone asked for immediate processing.
fn sleep_or_wake(state: &AppState, total: Duration) {
    let step = Duration::from_millis(500);
    let mut slept = Duration::ZERO;
    while slept < total {
        if state.engine_wake.swap(false, Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(step);
        slept += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity() -> PendingActivity {
        PendingActivity {
            id: 1,
            app_name: "Google Chrome".into(),
            window_title: "Vatsalya proposal - Claude - Google Chrome – Partha".into(),
            url: Some("https://claude.ai/chat/1".into()),
            started_at: 0,
            ended_at: 600_000,
        }
    }

    #[test]
    fn verification_drops_strings_not_in_source() {
        let text = "Budget of ₹35L including GST for Sikkim's IVF scheme, deadline 30 September 2026.";
        let raw = r#"{"keep":true,"kind":"chat","title":"Vatsalya proposal","summary":"Budget discussion.",
            "people":["Partha","Rahul"],"organizations":["Sikkim government","Claude"],
            "dates":["30 September 2026","2025"],"numbers":["₹35L","₹35 L","80+"],"confidence":0.9}"#;
        let m = build_memory(raw, &activity(), text, "test").unwrap();
        assert_eq!(m.people, vec!["Partha"], "from the window title; Rahul invented");
        assert_eq!(m.organizations, vec!["Claude"], "'Sikkim government' is not literal");
        assert_eq!(m.dates, vec!["30 September 2026"]);
        assert_eq!(m.numbers, vec!["₹35L"], "whitespace-normalised duplicate merged, 80+ dropped");
        assert_eq!(m.dropped, 5);
        assert!(m.keep && (m.confidence - 0.9).abs() < 1e-9);
    }

    #[test]
    fn tasks_are_kept_when_mostly_grounded() {
        let src = normalize("Tom said he will fix the auth bug by Monday and Priya handles the rollout next week.");
        let (kept, dropped) = verify_tasks(
            vec!["Tom to fix the auth bug by Monday".into(), "Priya handles the rollout next week".into(), "Order more coffee for the office".into(), "short".into()],
            &src,
        );
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert_eq!(dropped, 2);
    }

    #[test]
    fn invalid_json_is_an_error_and_empty_title_falls_back() {
        assert!(build_memory("not json", &activity(), "x", "m").is_err());
        let raw = r#"{"keep":false,"kind":"other","title":"  ","summary":"","confidence":2.0}"#;
        let m = build_memory(raw, &activity(), "x", "m").unwrap();
        assert_eq!(m.title, activity().window_title);
        assert_eq!(m.confidence, 1.0);
    }

    #[test]
    fn prompt_includes_context() {
        let p = prompt_for(&activity(), "hello");
        assert!(p.contains("Time spent: 10 min") && p.contains("claude.ai") && p.ends_with("hello"));
    }
}

// ── Ask ──────────────────────────────────────────────────────────────────

const ASK_SYSTEM: &str = "You are Lane, the memory of the person you are talking to. You have their memories of what \
they saw and did on their Mac, each with a time. Answer like a sharp colleague who was there: lead with the answer, in one \
or two natural sentences, then only the detail that matters. Speak to them as \"you\" and refer to time the way people do \
(\"last night\", \"on Tuesday\", \"about an hour ago\") using the times given. Use ONLY the memories provided; cite the ones \
you use inline as [1], [2]. When a FACTS block is given, take numbers, dates and names from it exactly as written; if two \
facts disagree, give the newest (or the one confirmed by you) and mention the older value with its date in a few words. If \
the memories do not contain the answer, say so in one sentence and mention the closest thing you do have. Answer exactly \
what was asked: when the question names a time window (today, yesterday, last week, a date), a person, a place or a thing, \
the first sentence either answers within that constraint or says plainly, in your own words for that question, that the \
memories hold nothing for it. Never lead with a figure or fact that belongs to a different period, person or thing; a page \
seen in the window that reports a total for another period, an old balance or a schedule is not what happened in the \
window. Related facts may follow, labelled for what they are. A question with no time word is about all your memories; do \
not narrow it to today. An order, invoice or cart marked pending, unpaid or waiting for payment is not money spent: say it \
is pending. Name a weekday or a date only when the memory's own time says it; the times given are the truth about when: each source \
begins with when it happened, and a date written inside the text is something the text talks about, not when the memory \
is from. When the same kind of thing happened more than once, answer with the most recent and say in a few words that \
there were earlier ones. \
Plain prose only: no headings, no bullet lists, no markdown, no \
preamble like \"Based on your memories\".";

/// What the question pins down, spelled out for the model and checked after
/// the answer: a time window, named people, a quantity.
#[derive(Default, Clone)]
pub struct Constraints {
    pub window: Option<(i64, i64)>,
    pub window_phrase: String,
    pub people: Vec<String>,
    pub quantity: bool,
    /// Facts that answer the question by themselves, already in the sources.
    pub leads: Vec<String>,
}

impl Constraints {
    pub fn any(&self) -> bool {
        self.window.is_some() || !self.people.is_empty() || self.quantity || !self.leads.is_empty()
    }
    /// One line for the prompt.
    pub fn line(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some((a, b)) = self.window {
            let days = (b - a) / 86_400_000;
            let same_span = if days >= 180 { "an annual or year-to-date total for this year is exactly what is asked" } else if days >= 25 { "a monthly total for this month is exactly what is asked" } else { "a total for a longer period (annual, monthly), an old balance or a plan is not what happened in the window" };
            parts.push(format!("the time window is {} ({} to {}): only what happened or was recorded inside it counts; {same_span}", if self.window_phrase.is_empty() { "as stated".to_string() } else { self.window_phrase.clone() }, day_of(a), day_of(b - 1)));
        }
        if !self.people.is_empty() {
            parts.push(format!("it is about {}: only what they said, did or were told counts, not what others did", self.people.join(" and ")));
        }
        if self.quantity {
            let span_rule = match self.window {
                Some((a, b)) if b - a > 2 * 86_400_000 => " The window spans many days: when a source states a total for that span (an annual or monthly spending figure), lead with it and mention single orders after; otherwise list the individual amounts with their dates and give their sum, saying it is the sum of what was captured.",
                _ => "",
            };
            parts.push(format!("it asks for an amount or count: give the figure a source states for what was asked, naming what it covers and its date; if the only figures are for a different period or thing, say so first and then name them.{span_rule}"));
        }
        if !self.leads.is_empty() {
            parts.push(format!("the sources state the answer outright: {}. The first sentence gives exactly this figure or fact with its citation; other amounts (single orders, invoices, parts) come after it, if at all", self.leads.join(" · ")));
        }
        if parts.is_empty() { String::new() } else { format!("CONSTRAINTS: {}. If the sources hold nothing that satisfies these, the first sentence says so.", parts.join("; ")) }
    }
}

/// The search form of a question: without the time phrases (the window
/// covers them) and, for amount questions, with the words pages use.
pub fn retrieval_terms(q: &str) -> String {
    let lower = q.to_lowercase();
    let mut out = String::new();
    let time_words = ["today", "yesterday", "tonight", "tomorrow", "this", "last", "week", "month", "year", "morning", "afternoon", "evening", "night", "recently", "ago"];
    for w in q.split_whitespace() {
        let bare: String = w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
        if bare.is_empty() || time_words.contains(&bare.as_str()) {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '.' && c != '-'));
    }
    let money = ["spend", "spent", "pay", "paid", "cost", "how much", "total", "price", "bill", "invoice", "charged"];
    if money.iter().any(|m| lower.contains(m)) {
        out.push_str(" spending spent paid total cost invoice order amount");
    }
    if out.trim().is_empty() { q.to_string() } else { out }
}

/// Lines of a list that take back what they list ("… — note: this expires
/// in August, so it does not count") are removed whole.
pub fn drop_disqualified_lines(text: &str) -> String {
    let bad = ["does not count", "doesn't count", "do not count", "not count", "does not qualify", "doesn't qualify", "not within", "outside the window", "outside this", "falls outside", "is not expiring this", "not this month", "not this week", "not this year", "before this month", "after this month", "so it is not", "so it does not"];
    text.lines()
        .filter(|l| !l.trim_start().to_lowercase().starts_with("note:") && !l.trim_start().starts_with("(Note"))
        .filter(|l| {
            let low = l.to_lowercase();
            !(low.contains("note:") || low.contains("however")) || !bad.iter().any(|b| low.contains(b))
        })
        .filter(|l| !bad.iter().any(|b| l.to_lowercase().contains(b)) || !l.contains('['))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The question without its time words and without the money synonyms.
pub fn retrieval_terms_plain(q: &str) -> String {
    let t = retrieval_terms(q);
    t.split(" spending spent paid total cost invoice order amount").next().unwrap_or(&t).to_string()
}

pub fn constraints_of(question: &str, now: i64, people: &[String]) -> Constraints {
    let q = question.to_lowercase();
    let phrases = ["today", "yesterday", "this morning", "this afternoon", "tonight", "last night", "this week", "last week", "this month", "last month", "this year", "last year"];
    let window_phrase = phrases.iter().find(|p| q.contains(*p)).map(|p| p.to_string()).unwrap_or_default();
    let window = date_range(&q, now);
    let quantity = ["how much", "how many", "what was the total", "what did i spend", "what did i pay", "what did it cost", "how long"].iter().any(|p| q.contains(p));
    Constraints { window, window_phrase, people: people.to_vec(), quantity, leads: Vec::new() }
}

/// A second, short pass over a finished answer: does its first sentence
/// respect the question's constraints, given the sources? If not, the
/// answer is rewritten so it does. Returns (answer, fixed).
fn scope_check(port: u16, question: &str, constraints: &Constraints, answer: &str, context: &str, leads: &str) -> (String, bool) {
    let schema = json!({"type": "object", "properties": {
        "ok": {"type": "boolean"},
        "reason": {"type": "string", "maxLength": 200},
        "rewrite": {"type": "string", "maxLength": 700}
    }, "required": ["ok", "reason", "rewrite"]});
    let who = WHO_NAME.lock().ok().and_then(|g| g.clone()).or_else(user_name).map(|n| format!(" The person asking is {n}: \"I\", \"me\", \"my\" and \"you\" all mean {n}, and anything the sources attribute to {n} is theirs.")).unwrap_or_default();
    let system = format!("You check whether an answer respects the question's explicit constraints.{who} Most answers are fine: ok=true unless the FIRST sentence clearly presents a figure, date or event from outside the stated time window, attributes to the named person something the sources attribute to someone else, or gives a different figure than a stated answer the constraints name (\"the sources state the answer outright\"). Hedging, missing detail or a different reading of a word are not violations; \"owe\" covers commitments and tasks, not only money. When ok=false, rewrite in the same voice, two or three sentences: first what the sources do or do not show for exactly what was asked, then the related fact labelled for what it is (say what it actually is; do not copy any template), keeping the [n] citations and every correct detail of the original. Otherwise rewrite is empty.");
    // The leads are what the sources state outright. They live at the end of
    // the context, so a plain truncation used to cut them off and the checker
    // would then "correct" a right answer into a wrong one.
    let compact: String = context.chars().take(4_000).collect();
    let user = format!("QUESTION: {question}
{}

ANSWER:
{answer}

{}SOURCES:
{compact}", constraints.line(), if leads.trim().is_empty() { String::new() } else { format!("WHAT THE SOURCES STATE OUTRIGHT (these are in the sources; an answer that uses them is correct):\n{leads}\n") });
    match runtime::chat_json(port, &system, &user, schema, 320) {
        Ok((raw, _)) => {
            let v: Value = serde_json::from_str(&repair_json(&raw)).unwrap_or(Value::Null);
            let ok = v["ok"].as_bool().unwrap_or(true);
            let rewrite = v["rewrite"].as_str().unwrap_or("").trim().to_string();
            if !ok && rewrite.chars().count() > 20 {
                log::info!("ask: scope check rewrote the answer: {}", v["reason"].as_str().unwrap_or(""));
                (rewrite, true)
            } else {
                (answer.to_string(), false)
            }
        }
        Err(e) => {
            log::warn!("ask: scope check failed: {e}");
            (answer.to_string(), false)
        }
    }
}

/// Previous turns of the same conversation, oldest first.
#[derive(Clone, serde::Deserialize)]
pub struct AskTurn {
    pub question: String,
    pub answer: String,
}

/// More context on machines that can afford it.
fn ask_top_k() -> u32 { if runtime::physical_ram_gb() >= 16 { 10 } else { 6 } }
fn ask_excerpt_chars() -> usize { if runtime::physical_ram_gb() >= 16 { 800 } else { 500 } }
const ASK_HISTORY_TURNS: usize = 3;

/// "yesterday 20:14", "today 09:02", "Tue 16 Sep 11:30", "3 Aug 2026".
pub fn relative_time(ms: i64, now_ms: i64) -> String {
    let offset = local_offset_secs();
    let day = |t: i64| (t / 1000 + offset).div_euclid(86_400);
    let (d, today) = (day(ms), day(now_ms));
    let clock = &format_time(ms)[11..];
    match today - d {
        0 => format!("today {clock}"),
        1 => format!("yesterday {clock}"),
        2..=6 => {
            let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(d.rem_euclid(7)) as usize];
            let date = &format_time(ms)[..10];
            format!("{weekday} {} {clock}", short_date(date))
        }
        _ => short_date(&format_time(ms)[..10]),
    }
}

/// "2026-09-16" → "16 Sep 2026"
fn short_date(iso: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let m: usize = iso[5..7].parse().unwrap_or(1);
    format!("{} {} {}", iso[8..10].trim_start_matches('0'), MONTHS[(m - 1).min(11)], &iso[..4])
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskSource {
    pub n: usize,
    pub card: Option<crate::store::MemoryCard>,
    pub file: Option<crate::store::FileHit>,
    pub task: Option<crate::store::Task>,
    pub entity: Option<crate::store::Entity>,
}

const ASK_FILES_K: u32 = 4;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskResult {
    pub id: u64,
    pub answer: String,
    pub sources: Vec<AskSource>,
    pub error: Option<String>,
    pub followups: Vec<String>,
    /// Figures and dates in the answer that do not occur in any source
    /// given to the model. Empty = every figure checked out.
    #[serde(default)]
    pub unverified: Vec<String>,
    /// Whether the check ran (false for prepared or cached answers).
    #[serde(default)]
    pub checked: bool,
    /// The first answer did not respect what was asked (a period, a person) and was rewritten.
    #[serde(default)]
    pub scope_fixed: bool,
}

/// Every number, amount and date in the answer must occur in the context
/// the model was given. Returns the ones that do not, as written.
pub fn unverified_figures(answer: &str, context: &str) -> Vec<String> {
    let norm = |s: &str| s.to_lowercase().replace([',', ' ', '\u{a0}'], "");
    let hay = norm(context);
    let mut out: Vec<String> = Vec::new();
    let months = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december", "jan", "feb", "mar", "apr", "jun", "jul", "aug", "sep", "sept", "oct", "nov", "dec"];
    let tokens: Vec<&str> = answer.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        // Citations ("[3]", "[1][2]", "[23][24]") are not figures.
        if tokens[i].contains('[') || tokens[i].contains(']') {
            i += 1;
            continue;
        }
        let t = tokens[i].trim_matches(|c: char| ",.;:()\"'".contains(c));
        let digits = t.chars().filter(|c| c.is_ascii_digit()).count();
        if digits >= 2 {
            // "35 lakh", "20 September", "2026/09/20", "₹1,20,000", "12%"
            let mut phrase = t.to_string();
            if i + 1 < tokens.len() {
                let next = tokens[i + 1].trim_matches(|c: char| ",.;:()[]\"'".contains(c)).to_lowercase();
                if ["lakh", "lakhs", "crore", "crores", "million", "billion", "k", "%"].contains(&next.as_str()) || months.contains(&next.as_str()) {
                    phrase = format!("{t} {}", tokens[i + 1].trim_matches(|c: char| ",.;:()[]\"'".contains(c)));
                }
            }
            let needle = norm(&phrase);
            let bare = norm(t);
            if !hay.contains(&needle) && !hay.contains(&bare) && !out.iter().any(|x| norm(x) == needle) {
                out.push(phrase);
            }
        }
        i += 1;
    }
    // A weekday the sources never name is an invention too ("on Tuesday"
    // for a meeting held today). The context carries weekday names only
    // where the times say them.
    let low = answer.to_lowercase();
    let ctx = context.to_lowercase();
    for day in ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"] {
        let named = low.split(|c: char| !c.is_alphabetic()).any(|w| w == day);
        if named && !ctx.contains(day) && !ctx.contains(&day[..3]) && !out.iter().any(|x| x.eq_ignore_ascii_case(day)) {
            let mut d = day.to_string();
            if let Some(f) = d.get_mut(0..1) { f.make_ascii_uppercase(); }
            out.push(d);
        }
    }
    out
}

pub fn format_time(ms: i64) -> String {
    // 2026-09-17 20:14 in local time, without pulling a date crate in.
    let secs = ms / 1000;
    let offset = local_offset_secs();
    let t = secs + offset;
    let (days, rem) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, (rem % 3600) / 60)
}

/// The Mac account's full name, so the model knows who "you" is.
/// What to call the person: their own answer first, the Mac account second.
pub fn user_name_with(state: &AppState) -> Option<String> {
    let chosen = crate::lock(&state.settings).display_name.trim().to_string();
    if !chosen.is_empty() {
        return Some(chosen);
    }
    user_name()
}

/// The chosen name, kept where the prompt builders can reach it without a
/// handle on the whole state.
pub static WHO_NAME: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn set_who(name: &str) {
    if let Ok(mut g) = WHO_NAME.lock() {
        let n = name.trim();
        *g = (!n.is_empty()).then(|| n.to_string());
    }
}

pub fn user_name() -> Option<String> {
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_gecos.is_null() {
            return None;
        }
        let gecos = std::ffi::CStr::from_ptr((*pw).pw_gecos).to_string_lossy();
        let name = gecos.split(',').next().unwrap_or("").trim().to_string();
        (!name.is_empty()).then_some(name)
    }
}

pub fn local_offset_secs() -> i64 {
    unsafe {
        let mut t: libc::time_t = 0;
        libc::time(&mut t);
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm.tm_gmtoff as i64
    }
}

/// Answer a question over memories. Streams tokens as `ask-token` events
/// and finishes with `ask-done`. Runs on its own thread.
const DRAFT_SYSTEM: &str = "You write on behalf of the person you are talking to, using ONLY their memories provided: emails, \
messages, summaries, replies. Match what they ask for (tone, length, recipient). Use the facts, names, numbers and dates \
from the memories; never invent details, and put [square brackets] around anything they must fill in. Cite the memories \
you drew on at the end as a single line: Sources: [1], [2]. Plain text, no markdown.";

pub fn ask(app: AppHandle, state: Arc<AppState>, id: u64, question: String, history: Vec<AskTurn>, draft: bool, conversation_id: Option<i64>) {
    std::thread::Builder::new()
        .name("ask".into())
        .spawn(move || {
            // The conversation's own turns are the history when the caller gave none.
            let history = if history.is_empty() {
                conversation_id.map(|c| conversation_history(&state, c)).unwrap_or_default()
            } else {
                history
            };
            state.ask_active.store(true, Ordering::Relaxed);
            let result = answer_with(&state, &question, &history, draft, |t| {
                let _ = app.emit("ask-token", serde_json::json!({"id": id, "token": t}));
            });
            state.ask_active.store(false, Ordering::Relaxed);
            let payload = match result {
                Ok((answer, sources)) => {
                    if let Some(c) = conversation_id {
                        let _ = crate::lock(&state.store).append_message(c, "assistant", &answer, &serde_json::to_string(&sources).unwrap_or_default(), crate::capture::now_ms());
                    }
                    let (checked, unverified) = take_check(&answer);
                    let scope_fixed = std::mem::replace(&mut *crate::lock(&LAST_SCOPE_FIX), false);
                    AskResult { id, answer, sources, error: None, followups: vec![], unverified, checked, scope_fixed }
                }
                Err(e) => AskResult { id, answer: String::new(), sources: vec![], error: Some(e), followups: vec![], unverified: vec![], checked: false, scope_fixed: false },
            };
            let ok = payload.error.is_none();
            let answer = payload.answer.clone();
            // The answer is done for the user now; suggestions arrive a moment later.
            let _ = app.emit("ask-done", payload);
            if ok {
                let followups = suggest_followups(&state, &question, &answer).unwrap_or_default();
                if !followups.is_empty() {
                    let _ = app.emit("ask-followups", serde_json::json!({"id": id, "followups": followups}));
                }
            }
        })
        .expect("spawn ask");
}

/// The last few completed turns of a saved conversation.
pub fn conversation_history(state: &AppState, conversation_id: i64) -> Vec<AskTurn> {
    let msgs = crate::lock(&state.store).conversation_messages(conversation_id).unwrap_or_default();
    let mut turns = Vec::new();
    let mut pending: Option<String> = None;
    for m in msgs {
        match m.role.as_str() {
            "user" => pending = Some(m.content),
            "assistant" => {
                if let Some(q) = pending.take() {
                    turns.push(AskTurn { question: q, answer: m.content });
                }
            }
            _ => {}
        }
    }
    let skip = turns.len().saturating_sub(ASK_HISTORY_TURNS);
    turns.into_iter().skip(skip).collect()
}

/// Answers prepared ahead of time (meeting prep after a reminder), keyed by
/// the exact question, good for two hours.
static PREPARED: std::sync::Mutex<Vec<(String, String, Vec<AskSource>, i64)>> = std::sync::Mutex::new(Vec::new());

fn prepared_answer(question: &str, now: i64) -> Option<(String, Vec<AskSource>)> {
    let mut cache = crate::lock(&PREPARED);
    cache.retain(|(_, _, _, at)| now - at < 2 * 3_600_000);
    cache.iter().find(|(q, _, _, _)| q == question).map(|(_, a, s, _)| (a.clone(), s.clone()))
}

pub fn answer_with(state: &AppState, question: &str, history: &[AskTurn], draft: bool, mut on_token: impl FnMut(&str)) -> Result<(String, Vec<AskSource>), String> {
    if history.is_empty() && !draft {
        if let Some((answer, sources)) = prepared_answer(question, crate::capture::now_ms()) {
            on_token(&answer);
            return Ok((answer, sources));
        }
    }
    let model = crate::lock(&state.settings).model.clone();
    let Backend::Bundled { port } = ensure_backend(state, &model)? else {
        return Err("Ask needs the bundled model runtime".into());
    };
    // A short follow-up ("who approved it?") carries no searchable words:
    // retrieve with the previous question added.
    let retrieval_query = match history.last() {
        Some(prev) if question.split_whitespace().count() <= 5 => format!("{} {}", prev.question, question),
        _ => question.to_string(),
    };
    // "Sarah" → "Sarah Khan"; the model still sees the question as asked.
    let retrieval_query = crate::lock(&state.store).alias_expand(&retrieval_query).unwrap_or(retrieval_query);
    // Time words are handled by the window, not the search; money questions
    // are asked in more words than pages use ("spend" vs "spending",
    // "invoice", "total"), so those are added for retrieval only.
    let retrieval_query = retrieval_terms(&retrieval_query);
    let intent = classify(question, crate::capture::now_ms());
    // Wide and compact for questions that gather ("which domains", "what
    // happened"), narrow and deep for questions that pinpoint. At 100K
    // memories the answer is never "read 10 of them": candidates come from
    // the indexes, and the model reads many short lines plus a few excerpts.
    let wide = matches!(intent.kind, IntentKind::List | IntentKind::Timeline | IntentKind::Synthesis | IntentKind::Actions);
    let top_k = if wide { ask_top_k() * 3 } else { ask_top_k() };
    let excerpts = if wide { 3 } else { ask_top_k().min(5) as usize };
    let qvec = embed_query(state, &retrieval_query);
    let mut cards = crate::lock(&state.store)
        .search_memories_in(&retrieval_query, qvec.as_deref(), true, top_k, intent.window)
        .map_err(|e| e.to_string())?;
    // A follow-up keeps the previous turn's memories in front, so "and the
    // deadline?" is answered from the same sources without a new hunt.
    if let Some(prev) = history.last() {
        if let Some(kept) = crate::lock(&LAST_CARDS).as_ref().filter(|(q, _)| q == &prev.question).map(|(_, c)| c.clone()) {
            let mut merged = kept;
            for c in cards {
                if !merged.iter().any(|x| x.id == c.id) {
                    merged.push(c);
                }
            }
            merged.truncate(top_k as usize);
            cards = merged;
        }
    }
    // Second hop through the names the first hop surfaced.
    if let Some(hop) = hop_query(question, &cards) {
        let hop_q = format!("{retrieval_query} {hop}");
        let hop_vec = embed_query(state, &hop_q);
        if let Ok(more) = crate::lock(&state.store).search_memories_in(&hop_q, hop_vec.as_deref(), true, top_k, intent.window) {
            for c in more {
                if cards.len() >= top_k as usize {
                    break;
                }
                if !cards.iter().any(|x| x.id == c.id || x.ids.contains(&c.id)) {
                    cards.push(c);
                }
            }
        }
    }
    if matches!(intent.kind, IntentKind::Timeline) {
        cards.sort_by_key(|c| c.started_at);
    }
    // "How much…": a memory that carries figures answers before one that
    // merely mentions the topic. Stable, so the search order still holds
    // within each group.
    if constraints_of(question, crate::capture::now_ms(), &[]).quantity {
        cards.sort_by_key(|c| c.numbers.is_empty());
    }
    let files = crate::lock(&state.store).search_files(&retrieval_query, qvec.as_deref(), ASK_FILES_K).unwrap_or_default();
    let any_fact = crate::lock(&state.store).search_facts(&retrieval_query, 1).map(|v| !v.is_empty()).unwrap_or(false);
    if cards.is_empty() && files.is_empty() && !any_fact && crate::lock(&state.store).entities_named_in(question, 1).map(|v| v.is_empty()).unwrap_or(true) {
        return Ok(("I don't have any memories or documents about that yet.".into(), vec![]));
    }
    let now = crate::capture::now_ms();
    let mut context = String::new();
    /// What the sources say outright, kept apart so the checker always sees it.
    let mut leads = String::new();
    let mut sources = Vec::new();
    for (i, c) in cards.iter().enumerate() {
        let n = i + 1;
        let facts = crate::lock(&state.store).facts_for(&c.ids).map(|fs| fs.iter().take(4).map(|f| format!("{} {}: {}", f.subject, f.attribute, f.value)).collect::<Vec<_>>().join("; ")).unwrap_or_default();
        if i < excerpts {
            let excerpt = crate::lock(&state.store).activity_text(c.activity_id, ask_excerpt_chars()).unwrap_or_default();
            context.push_str(&format!(
                "[{n}] {} · {} · {}\nTitle: {}\nSummary: {}\nPeople: {} · Organizations: {} · Projects: {} · Dates: {} · Numbers: {}{}\nExcerpt: {}\n\n",
                relative_time(c.started_at, now),
                c.app_name,
                c.url.clone().unwrap_or_default(),
                c.title,
                c.summary,
                c.people.join(", "),
                c.organizations.join(", "),
                c.projects.join(", "),
                c.dates.join(", "),
                c.numbers.join(", "),
                if facts.is_empty() { String::new() } else { format!("\nFacts: {facts}") },
                excerpt.replace('\n', " ")
            ));
        } else {
            // One compact line each: enough to list, count and compare.
            context.push_str(&format!(
                "[{n}] {} · {} · {} — {}{}{}\n",
                relative_time(c.started_at, now),
                c.app_name,
                c.title,
                c.summary.chars().take(220).collect::<String>(),
                if c.numbers.is_empty() && c.dates.is_empty() { String::new() } else { format!(" ({} {})", c.dates.join(", "), c.numbers.join(", ")) },
                if facts.is_empty() { String::new() } else { format!(" · {facts}") }
            ));
        }
        sources.push(AskSource { n, card: Some(c.clone()), file: None, task: None, entity: None });
    }
    if cards.len() > excerpts {
        context.push('\n');
    }
    // Lines that answer the question by themselves (a direct fact, a stated
    // total); repeated right before the question, where a small model reads
    // most carefully.
    let mut lead_lines: Vec<String> = Vec::new();
    // Stated values, with their dates; disagreements are shown, newest first.
    // Two searches: the question's own words first (a stated total such as
    // "annual spending" must not be crowded out by the retrieval synonyms),
    // then the expanded query.
    let facts = {
        let store = crate::lock(&state.store);
        let cap = if wide { 16 } else { 8 };
        let mut facts = store.search_facts(&retrieval_terms_plain(question), cap).unwrap_or_default();
        for f in store.search_facts(&retrieval_query, cap).unwrap_or_default() {
            if !facts.iter().any(|(x, _, _)| x.id == f.0.id) {
                facts.push(f);
            }
        }
        facts.truncate(cap as usize + 4);
        facts
    };
    if !facts.is_empty() {
        let mut block = String::from("FACTS (exact values from the memories; when two disagree, prefer the newest and mention the older one with its date; \"yours\" means the user's own, \"someone else's\" means seen but not theirs; \"proposed\" is not \"agreed\"):\n");
        let mut extra_sources: Vec<i64> = Vec::new();
        for (f, activity_id, title) in &facts {
            let n = match sources.iter().position(|s| s.card.as_ref().map_or(false, |c| c.ids.contains(&f.memory_id))) {
                Some(i) => i + 1,
                None => {
                    if let Ok(Some(card)) = crate::lock(&state.store).memory_by_id(f.memory_id) {
                        let n = sources.len() + 1;
                        sources.push(AskSource { n, card: Some(card), file: None, task: None, entity: None });
                        extra_sources.push(*activity_id);
                        n
                    } else {
                        0
                    }
                }
            };
            let _ = title;
            let tag = match (f.owner.as_str(), f.stance.as_str()) {
                ("mine", "stated") => " · yours".to_string(),
                ("theirs", "stated") => " · someone else's".to_string(),
                (o, st) => format!(" · {}{}", if o == "mine" { "yours, " } else if o == "theirs" { "someone else's, " } else { "" }, st),
            };
            block.push_str(&format!("- {} — {}: {} (as of {}{tag}, [{n}]{})\n", f.subject, f.attribute, f.value, day_of(f.as_of), if f.origin == "user" { ", confirmed by you" } else { "" }));
            for c in &f.conflicts {
                block.push_str(&format!("  · disagrees with: {} (as of {}, from \"{}\")\n", c.value, day_of(c.as_of), c.title));
            }
        }
        context.push_str(&block);
        context.push('\n');
        // "What is the Dynadot annual spending?": a fact whose subject and
        // attribute are all in the question is the answer itself; the model
        // is told so rather than left to pick invoices from the memories.
        {
            const STOP: &[&str] = &["what", "which", "when", "where", "who", "how", "why", "the", "and", "for", "with", "from", "that", "this", "did", "does", "was", "were", "are", "have", "has", "about", "much", "many", "you", "your", "our", "into", "than", "then", "there", "some", "all", "also", "just", "not"];
            let q_words: std::collections::HashSet<String> = question.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() > 2 && !STOP.contains(w)).map(|w| w.trim_end_matches('s').to_string()).collect();
            let mut direct: Vec<String> = Vec::new();
            for (f, _, _) in &facts {
                let key: Vec<String> = format!("{} {}", f.subject, f.attribute).to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() > 2 && !STOP.contains(w)).map(|w| w.trim_end_matches('s').to_string()).collect();
                // "Dynadot annual spending" should match the fact "Dynadot
                // account — annual spending": asking for every word back is
                // too strict, so most of the key, and at least two words, is
                // enough. One word in common is not (balance vs spending).
                let hit = key.iter().filter(|w| q_words.contains(*w)).count();
                if key.len() >= 2 && hit >= 2 && hit * 3 >= key.len() * 2 {
                    let n = sources.iter().position(|s| s.card.as_ref().map_or(false, |c| c.ids.contains(&f.memory_id))).map(|i| i + 1).unwrap_or(0);
                    direct.push(format!("{} — {}: {} (as of {}) [{}]", f.subject, f.attribute, f.value, day_of(f.as_of), n));
                }
            }
            if !direct.is_empty() {
                log::info!("ask: direct facts: {}", direct.join(" | "));
                let block = format!("FACTS THAT ANSWER THE QUESTION DIRECTLY (lead with these, exactly as written):\n- {}\n\n", direct.join("\n- "));
                leads.push_str(&block);
                context.push_str(&block);
                lead_lines.extend(direct);
            }
        }
        // "How much this year": a fact that states a total for that very
        // span (annual spending, monthly total) is the answer, and the
        // model is told so rather than left to notice it among invoices.
        let span_constraints = constraints_of(question, now, &[]);
        if span_constraints.quantity {
            if let Some((a, b)) = span_constraints.window {
                let days = (b - a) / 86_400_000;
                let span_words: &[&str] = if days >= 180 { &["annual", "yearly", "year"] } else if days >= 25 { &["monthly", "month"] } else if days >= 6 { &["weekly", "week"] } else { &[] };
                if !span_words.is_empty() {
                    let mut lead: Vec<String> = Vec::new();
                    for (f, _, _) in &facts {
                        let a_l = format!("{} {}", f.subject, f.attribute).to_lowercase();
                        if span_words.iter().any(|w| a_l.contains(w)) && f.value.chars().any(|c| c.is_ascii_digit()) {
                            let n = sources.iter().position(|s| s.card.as_ref().map_or(false, |c| c.ids.contains(&f.memory_id))).map(|i| i + 1).unwrap_or(0);
                            lead.push(format!("{} — {}: {} (as of {}) [{}]", f.subject, f.attribute, f.value, day_of(f.as_of), n));
                        }
                    }
                    if !lead.is_empty() {
                        log::info!("ask: stated total for the span: {}", lead.join(" | "));
                        let block = format!("THE ANSWER TO THE AMOUNT ASKED IS THIS STATED TOTAL (say it first, in one sentence, with its date and citation; single invoices and orders below are parts of it and come after, if at all):\n- {}\n\n", lead.join("\n- "));
                        leads.push_str(&block);
                        context.push_str(&block);
                        lead_lines.extend(lead);
                    }
                }
            }
        }
    }
    // Who someone is, from everything about them.
    let entities = crate::lock(&state.store).entities_named_in(question, 2).unwrap_or_default();
    for e in &entities {
        if e.mentions < 2 {
            continue;
        }
        let n = sources.len() + 1;
        let brief = crate::lock(&state.store).entity_brief(e.id, 5).unwrap_or_default();
        context.push_str(&format!("[{n}] {brief}\n"));
        sources.push(AskSource { n, card: None, file: None, task: None, entity: Some(e.clone()) });
    }
    // Open commitments that match.
    let mut tasks = crate::lock(&state.store).search_tasks(&retrieval_query, 4).unwrap_or_default();
    // "What do I owe Sarah": the commitments live in memories that mention
    // her, whether or not her name is in the task text.
    if matches!(intent.kind, IntentKind::Actions | IntentKind::Person) {
        for e in &entities {
            for t in crate::lock(&state.store).tasks_for_entity(e.id, 6).unwrap_or_default() {
                if !tasks.iter().any(|x| x.id == t.id) {
                    tasks.push(t);
                }
            }
        }
        // "What do I owe anyone": no name, no matching words, but the open
        // commitments are the answer. Newest first.
        if matches!(intent.kind, IntentKind::Actions) && entities.is_empty() {
            for t in crate::lock(&state.store).list_tasks("open", 10).unwrap_or_default() {
                if !tasks.iter().any(|x| x.id == t.id) {
                    tasks.push(t);
                }
            }
        }
        tasks.truncate(10);
    }
    for t in &tasks {
        let n = sources.len() + 1;
        context.push_str(&format!("[{n}] Open task: {} (from \"{}\", {})\n\n", t.text, t.title, relative_time(t.started_at, now)));
        sources.push(AskSource { n, card: None, file: None, task: Some(t.clone()), entity: None });
    }
    let wants_files = !matches!(intent.kind, IntentKind::Actions) || question.to_lowercase().contains("file") || question.to_lowercase().contains("document");
    for f in files.iter().filter(|_| wants_files) {
        let n = sources.len() + 1;
        let excerpt = crate::lock(&state.store).file_chunk_text(f.chunk_id, 600).unwrap_or_default();
        context.push_str(&format!(
            "[{n}] Document on this Mac: {} · {} · modified {}\nExcerpt: {}\n\n",
            f.name,
            f.path,
            relative_time(f.mtime, now),
            excerpt.replace('\n', " ")
        ));
        sources.push(AskSource { n, card: None, file: Some(f.clone()), task: None, entity: None });
    }
    let mut convo = String::new();
    for t in history.iter().rev().take(ASK_HISTORY_TURNS).collect::<Vec<_>>().into_iter().rev() {
        let a: String = t.answer.chars().take(600).collect();
        convo.push_str(&format!("They asked: {}\nYou answered: {}\n\n", t.question, a));
    }
    let who = WHO_NAME.lock().ok().and_then(|g| g.clone()).or_else(user_name).map(|n| format!(" The person you are talking to is {n}; when the memories mention them, that is \"you\".")).unwrap_or_default();
    let asked_people: Vec<String> = {
        let q = question.to_lowercase();
        entities.iter().filter(|e| e.kind == "person" && q.contains(&e.name.to_lowercase())).map(|e| e.name.clone()).collect()
    };
    let mut constraints = constraints_of(question, now, &asked_people);
    constraints.leads = lead_lines.clone();
    let constraint_line = constraints.line();
    if !constraint_line.is_empty() {
        log::info!("ask: {}", constraint_line.chars().take(200).collect::<String>());
    }
    let user = format!(
        "Now: {} ({}).{who}\n\nMEMORIES:\n{context}{}QUESTION: {question}\n{}\nANSWER STYLE: {}{}",
        format_time(now),
        relative_time(now, now).split(' ').next().unwrap_or("today"),
        if convo.is_empty() { String::new() } else { format!("EARLIER IN THIS CONVERSATION:\n{convo}") },
        if constraint_line.is_empty() { String::new() } else { format!("{constraint_line}\n") },
        intent.instruction,
        intent.window.map(|(a, b)| format!(" Only memories between {} and {} were considered.", day_of(a), day_of(b - 1))).unwrap_or_default()
    );
    let budget = if draft { 700 } else if matches!(intent.kind, IntentKind::Timeline | IntentKind::List | IntentKind::Synthesis) { 650 } else { 400 };
    let text = runtime::chat_stream(port, if draft { DRAFT_SYSTEM } else { ASK_SYSTEM }, &user, budget, on_token)?;
    // Keep only sources the answer actually cites; drop citations out of range.
    let hedged = {
        let first = text.trim_start().to_lowercase();
        ["nothing", "no ", "none", "the sources do not", "your memories do not", "i don't have", "i do not have", "there is no", "there's no"].iter().any(|p| first.starts_with(p))
    };
    let text = if !draft && constraints.any() && !hedged {
        let (fixed, changed) = scope_check(port, question, &constraints, &text, &context, &leads);
        *crate::lock(&LAST_SCOPE_FIX) = changed;
        fixed
    } else {
        *crate::lock(&LAST_SCOPE_FIX) = false;
        text
    };
    // A list answer sometimes writes an item and then disqualifies it in the
    // same line; such lines are not answers and go.
    let text = if matches!(intent.kind, IntentKind::List) { drop_disqualified_lines(&text) } else { text };
    let cited: std::collections::BTreeSet<usize> = citations(&text).into_iter().filter(|n| *n >= 1 && *n <= sources.len()).collect();
    let answer = strip_markdown(&strip_bad_citations(&text, sources.len()));
    let sources = if cited.is_empty() { sources } else { sources.into_iter().filter(|s| cited.contains(&s.n)).collect() };
    *crate::lock(&LAST_CARDS) = Some((question.to_string(), cards.clone()));
    let unverified = unverified_figures(&answer, &context);
    if !unverified.is_empty() {
        log::info!("ask: figures not in sources: {:?}", unverified);
    }
    *crate::lock(&LAST_CHECK) = Some((answer.clone(), unverified));
    log::info!("ask: {} chars, {} sources", answer.len(), sources.len());
    Ok((answer, sources))
}

/// The memories the last answer used, keyed by its question, for follow-ups.
static LAST_CARDS: std::sync::Mutex<Option<(String, Vec<crate::store::MemoryCard>)>> = std::sync::Mutex::new(None);

/// The last answer's verification, picked up by `ask` for the result.
static LAST_CHECK: std::sync::Mutex<Option<(String, Vec<String>)>> = std::sync::Mutex::new(None);
/// Whether the scope check rewrote the last answer.
pub static LAST_SCOPE_FIX: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

pub fn take_check(answer: &str) -> (bool, Vec<String>) {
    let mut g = crate::lock(&LAST_CHECK);
    match g.take() {
        Some((a, u)) if a == answer => (true, u),
        other => {
            *g = other;
            (false, vec![])
        }
    }
}

const SCREEN_SYSTEM: &str = "You are Lane, the memory of the person you are talking to. Source [S] is the text on their \
screen right now (the window that was in front when they asked); other numbered sources are their own memories and \
documents. Questions like \"this\", \"here\", \"summarise\" or \"reply\" refer to [S]. Answer like a sharp colleague: the \
answer first, then only the detail that matters; use memories to add what the screen does not say (who these people are, \
what was agreed before, exact figures). Cite [S] and the memories you use inline. If asked for a reply or draft, write it \
ready to paste, with [square brackets] around anything to fill in. Plain prose, no markdown, no preamble.";

/// Answer over the window that was in front when the overlay opened, plus
/// the memories that relate to it.
pub fn answer_about_screen(state: &AppState, question: &str, history: &[AskTurn], on_token: impl FnMut(&str)) -> Result<(String, Vec<AskSource>), String> {
    let screen = crate::lock(&state.screen).clone().ok_or("Nothing readable was on screen when Lane opened. Bring the window in front and press the shortcut again.")?;
    let model = crate::lock(&state.settings).model.clone();
    let Backend::Bundled { port } = ensure_backend(state, &model)? else {
        return Err("Ask needs the bundled model runtime".into());
    };
    let now = crate::capture::now_ms();
    let max_screen = if runtime::physical_ram_gb() >= 16 { 9_000 } else { 6_000 };
    let screen_text: String = screen.text.chars().take(max_screen).collect();
    let mut context = format!(
        "[S] ON SCREEN NOW · {} · {}{}\n{}\n\n",
        screen.app_name,
        screen.window_title,
        screen.url.as_deref().map(|u| format!(" · {u}")).unwrap_or_default(),
        screen_text.replace('\n', " ")
    );
    // Memories that relate: the question, the title and the names on screen.
    let retrieval = format!("{question} {}", screen.window_title);
    let retrieval = crate::lock(&state.store).alias_expand(&retrieval).unwrap_or(retrieval);
    let qvec = embed_query(state, &retrieval);
    let cards = crate::lock(&state.store).search_memories_in(&retrieval, qvec.as_deref(), true, 4, None).unwrap_or_default();
    let mut sources = Vec::new();
    for (i, c) in cards.iter().enumerate() {
        let n = i + 1;
        context.push_str(&format!("[{n}] {} · {} · {}\nSummary: {}\nPeople: {} · Organizations: {} · Numbers: {}\n\n", relative_time(c.started_at, now), c.app_name, c.title, c.summary, c.people.join(", "), c.organizations.join(", "), c.numbers.join(", ")));
        sources.push(AskSource { n, card: Some(c.clone()), file: None, task: None, entity: None });
    }
    let facts = crate::lock(&state.store).search_facts(&retrieval, 6).unwrap_or_default();
    if !facts.is_empty() {
        context.push_str("FACTS FROM MEMORY:\n");
        for (f, _, title) in &facts {
            context.push_str(&format!("- {} — {}: {} (as of {}, from \"{}\")\n", f.subject, f.attribute, f.value, day_of(f.as_of), title));
        }
        context.push('\n');
    }
    let mut convo = String::new();
    for t in history.iter().rev().take(ASK_HISTORY_TURNS).collect::<Vec<_>>().into_iter().rev() {
        convo.push_str(&format!("They asked: {}\nYou answered: {}\n\n", t.question, t.answer.chars().take(600).collect::<String>()));
    }
    let who = user_name().map(|n| format!(" The person you are talking to is {n}.")).unwrap_or_default();
    let user = format!("Now: {}.{who}\n\n{context}{}QUESTION: {question}", format_time(now), if convo.is_empty() { String::new() } else { format!("EARLIER IN THIS CONVERSATION:\n{convo}") });
    touch_models();
    let text = runtime::chat_stream(port, SCREEN_SYSTEM, &user, 600, on_token)?;
    let cited: std::collections::BTreeSet<usize> = citations(&text).into_iter().filter(|n| *n >= 1 && *n <= sources.len()).collect();
    let answer = strip_markdown(&strip_bad_citations(&text.replace("[S]", "\u{0}S\u{0}"), sources.len())).replace("\u{0}S\u{0}", "[S]");
    let sources = if cited.is_empty() { sources } else { sources.into_iter().filter(|s| cited.contains(&s.n)).collect() };
    log::info!("ask-screen: {} chars over {} on-screen chars, {} sources", answer.len(), screen_text.len(), sources.len());
    Ok((answer, sources))
}

/// The prompt asks for plain prose; the model still slips in emphasis and
/// code marks. Remove them without touching the text itself.
pub fn strip_markdown(text: &str) -> String {
    let mut out = text.replace("**", "").replace('`', "");
    // Single '*' used for emphasis around words: *word*.
    while let Some(i) = out.find('*') {
        out.remove(i);
    }
    out.lines().map(|l| l.trim_start_matches("- ").trim_start_matches("• ")).collect::<Vec<_>>().join("\n")
}

/// Three short questions the user might ask next, from the answer just
/// given. One small structured call; never invents facts, only questions.
fn suggest_followups(state: &AppState, question: &str, answer: &str) -> Result<Vec<String>, String> {
    let model = crate::lock(&state.settings).model.clone();
    let Backend::Bundled { port } = ensure_backend(state, &model)? else { return Ok(vec![]) };
    let schema = json!({"type": "object", "properties": {"questions": {"type": "array", "items": {"type": "string"}, "maxItems": 3}}, "required": ["questions"]});
    let user = format!("They asked: {question}\nYou answered: {}\n\nSuggest 3 short follow-up questions they might naturally ask next, each under 10 words, about details, people, next steps or timing mentioned in the answer.", answer.chars().take(1200).collect::<String>());
    let (raw, _) = runtime::chat_json(port, "You suggest follow-up questions for a personal memory assistant. Plain questions only.", &user, schema, 120)?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    Ok(v["questions"].as_array().map(|a| a.iter().filter_map(|q| q.as_str()).map(|q| q.trim().to_string()).filter(|q| !q.is_empty()).take(3).collect()).unwrap_or_default())
}


// ── Question intent ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentKind {
    Factual,
    Temporal,
    List,
    Person,
    Timeline,
    Synthesis,
    Actions,
}

#[derive(Debug, Clone)]
pub struct Intent {
    pub kind: IntentKind,
    /// Memories must fall in this window when set.
    pub window: Option<(i64, i64)>,
    pub instruction: &'static str,
}

/// What kind of answer the question wants, and the time span it names.
/// Regex-free and instant, so it costs nothing per question.
pub fn classify(question: &str, now: i64) -> Intent {
    let q = question.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| q.contains(w));
    let starts = |words: &[&str]| words.iter().any(|w| q.trim_start().starts_with(w));
    let kind = if starts(&["who is", "who's", "who was", "who are"]) || has(&["tell me about ", "what do i know about ", "profile of "]) {
        IntentKind::Person
    } else if has(&["what happened", "history of", "timeline", "how did", "how has", "progress on", "status of", "where are we", "where do we stand"]) {
        IntentKind::Timeline
    } else if has(&["what should i", "follow up", "to do", "todo", "pending", "waiting on", "owe", "promised", "next step", "action item"]) {
        IntentKind::Actions
    } else if starts(&["list", "all the", "which ", "how many", "every "]) || has(&[" list ", " all the ", "everything about", "everyone "]) {
        IntentKind::List
    } else if starts(&["summarize", "summarise", "sum up", "brief me", "recap", "catch me up"]) || has(&["summary of", "overview of", "in short"]) {
        IntentKind::Synthesis
    } else if starts(&["when", "what time", "how long ago", "last time"]) || has(&["when did", "when was", "when is", "what day"]) {
        IntentKind::Temporal
    } else {
        IntentKind::Factual
    };
    let instruction = match kind {
        IntentKind::Factual => "Give the specific answer first (exact number, date or name), then one sentence of context.",
        IntentKind::Temporal => "Lead with the date and time (and how long ago), then what happened in a sentence.",
        IntentKind::List => "A short list is fine here: one line per item with the detail that qualifies it (a date, an amount), ending with its citation, in date order when there are dates. Decide what qualifies before writing: only items that meet the question's condition go in, and nothing else; no notes, asides or corrections after the list. Be complete across the memories given, no padding.",
        IntentKind::Person => "Describe who they are to the user: role, organisation, what you worked on together, the latest contact, anything open.",
        IntentKind::Timeline => "Tell it in order, oldest to newest, with dates, in a few sentences; end with where it stands now.",
        IntentKind::Synthesis => "Pull the memories together into a short synthesis; lead with what matters most.",
        IntentKind::Actions => "\"Owe\", \"pending\" and \"follow up\" mean commitments and promises (things to send, confirm, call, deliver), not money unless money is named. Answer from the open tasks and the memories: list what is open or owed, who to, and by when if known, one line each with citations; say when nothing is pending.",
    };
    Intent { kind, window: date_range(&q, now), instruction }
}

/// "yesterday", "today", "this week", "last week", "last month", "in
/// august", "on tuesday", "last 3 days", "3 days ago" → [since, until).
pub fn date_range(q: &str, now: i64) -> Option<(i64, i64)> {
    const DAY: i64 = 86_400_000;
    let (today, tomorrow) = day_bounds(now);
    // Calendar spans: this/last month and year, from the first day.
    if q.contains("this year") || q.contains("last year") || q.contains("this month") || q.contains("last month") {
        let offset = local_offset_secs();
        let secs = now / 1000 + offset;
        let days = secs.div_euclid(86_400);
        // Civil date from days since epoch (Howard Hinnant's algorithm).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        let (yy, mm) = (y, m);
        let last = q.contains("last year") || q.contains("last month");
        if q.contains("year") {
            let start_y = if last { yy - 1 } else { yy };
            let a = local_midnight(start_y, 1, 1)?;
            let b = if last { local_midnight(yy, 1, 1)? } else { tomorrow };
            return Some((a, b));
        }
        let (sy, sm) = if last { if mm == 1 { (yy - 1, 12) } else { (yy, mm - 1) } } else { (yy, mm) };
        let a = local_midnight(sy, sm, 1)?;
        let b = if last { local_midnight(yy, mm, 1)? } else { tomorrow };
        return Some((a, b));
    }
    if q.contains("yesterday") {
        return Some((today - DAY, today));
    }
    if q.contains("today") || q.contains("this morning") || q.contains("this afternoon") {
        return Some((today, tomorrow));
    }
    if q.contains("last night") {
        return Some((today - DAY + 17 * 3_600_000, today + 4 * 3_600_000));
    }
    if q.contains("this week") {
        let monday = today - weekday_index(today) * DAY;
        return Some((monday, tomorrow));
    }
    if q.contains("last week") {
        let monday = today - weekday_index(today) * DAY;
        return Some((monday - 7 * DAY, monday));
    }
    if q.contains("this month") {
        return Some((month_start(today), tomorrow));
    }
    if q.contains("last month") {
        let start = month_start(today);
        return Some((month_start(start - DAY), start));
    }
    for (i, w) in ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"].iter().enumerate() {
        if q.contains(&format!("on {w}")) || q.contains(&format!("last {w}")) || q.contains(&format!("{w}'s")) {
            let back = (weekday_index(today) - i as i64).rem_euclid(7);
            let back = if back == 0 { 7 } else { back };
            let day = today - back * DAY;
            return Some((day, day + DAY));
        }
    }
    for (i, m) in ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"].iter().enumerate() {
        if q.contains(&format!("in {m}")) || q.contains(&format!("during {m}")) {
            let (y, cm, _) = ymd(today);
            let year = if (i as i64 + 1) > cm { y - 1 } else { y };
            let start = ms_of(year, i as i64 + 1, 1);
            let end = if i == 11 { ms_of(year + 1, 1, 1) } else { ms_of(year, i as i64 + 2, 1) };
            return Some((start, end));
        }
    }
    // "last 3 days", "past 2 weeks", "3 days ago"
    let words: Vec<&str> = q.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        if (*w == "last" || *w == "past") && i + 2 < words.len() {
            if let Ok(n) = words[i + 1].parse::<i64>() {
                let unit = words[i + 2].trim_end_matches(',').trim_end_matches('s');
                let span = match unit { "day" => DAY, "week" => 7 * DAY, "month" => 30 * DAY, "hour" => 3_600_000, _ => 0 };
                if span > 0 {
                    return Some((now - n * span, tomorrow));
                }
            }
        }
        if *w == "ago" && i >= 2 {
            if let Ok(n) = words[i - 2].parse::<i64>() {
                let unit = words[i - 1].trim_end_matches('s');
                let span = match unit { "day" => DAY, "week" => 7 * DAY, "month" => 30 * DAY, _ => 0 };
                if span > 0 {
                    let day = day_bounds(now - n * span).0;
                    return Some((day, day + if unit == "day" { DAY } else { span }));
                }
            }
        }
    }
    None
}

/// 0 = Monday … 6 = Sunday, for a local-day start.
fn weekday_index(day_start_ms: i64) -> i64 {
    // 1970-01-01 was a Thursday (index 3).
    let days = (day_start_ms + local_offset_secs() * 1000).div_euclid(86_400_000);
    (days + 3).rem_euclid(7)
}

fn ymd(ms: i64) -> (i64, i64, i64) {
    let d = day_of(ms);
    let mut it = d.split('-').map(|x| x.parse::<i64>().unwrap_or(1));
    (it.next().unwrap_or(1970), it.next().unwrap_or(1), it.next().unwrap_or(1))
}

fn month_start(day_start_ms: i64) -> i64 {
    let (y, m, _) = ymd(day_start_ms);
    ms_of(y, m, 1)
}

/// Local midnight of a civil date; None for an impossible date.
pub fn local_midnight(y: i64, m: i64, d: i64) -> Option<i64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let ms = ms_of(y, m, d);
    // Reject 31 February and friends: the round trip must land on the same day.
    (day_of(ms) == format!("{y:04}-{m:02}-{d:02}")).then_some(ms)
}

/// Local midnight of a civil date, using the same offset the app uses for days.
fn ms_of(y: i64, m: i64, d: i64) -> i64 {
    let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400_000 - local_offset_secs() * 1000
}

/// Add a second retrieval hop: names in the best cards that the question
/// did not mention. "What did Sarah say about the tender?" pulls in the
/// tender memories that never name Sarah.
fn hop_query(question: &str, cards: &[crate::store::MemoryCard]) -> Option<String> {
    let lower = question.to_lowercase();
    let mut names: Vec<String> = Vec::new();
    for c in cards.iter().take(2) {
        for n in c.people.iter().chain(c.organizations.iter()).chain(c.projects.iter()) {
            if !lower.contains(&n.to_lowercase()) && !names.contains(n) && n.chars().count() >= 3 {
                names.push(n.clone());
            }
        }
    }
    names.truncate(3);
    if names.is_empty() { None } else { Some(names.join(" ")) }
}

// ── Today: the daily recap ───────────────────────────────────────────────

const RECAP_SYSTEM: &str = "You write a short morning briefing for the person you are talking to, from their own memories of the \
previous day: what they worked on, decisions and numbers that came up, open commitments, and people they dealt with. Speak \
to them as \"you\". Lead with the one or two things that mattered most. Cite memories inline as [1], [2]. Four short \
paragraphs at most, plain prose, no headings, no bullet lists, no markdown, no preamble. If the day was quiet, say so in \
one line.";

/// Local day string for a timestamp: "2026-09-17".
pub fn day_of(ms: i64) -> String {
    format_time(ms)[..10].to_string()
}

/// [start, end) of the local day containing `ms`.
pub fn day_bounds(ms: i64) -> (i64, i64) {
    let offset = local_offset_secs() * 1000;
    let start = (ms + offset).div_euclid(86_400_000) * 86_400_000 - offset;
    (start, start + 86_400_000)
}

/// Generate (or return the cached) recap for the day containing `ms`.
pub fn recap(state: &AppState, ms: i64, force: bool, mut on_token: impl FnMut(&str)) -> Result<(String, Vec<AskSource>), String> {
    let day = day_of(ms);
    if !force {
        if let Some((text, _, sources)) = crate::lock(&state.store).recap(&day).map_err(|e| e.to_string())? {
            let sources: Vec<AskSource> = serde_json::from_str(&sources).unwrap_or_default();
            on_token(&text);
            return Ok((text, sources));
        }
    }
    let (start, end) = day_bounds(ms);
    let (cards, tasks, apps) = {
        let store = crate::lock(&state.store);
        (
            store.memories_between(start, end, 14).map_err(|e| e.to_string())?,
            store.tasks_between(start, end).map_err(|e| e.to_string())?,
            store.time_by_app(start, end).map_err(|e| e.to_string())?,
        )
    };
    if cards.is_empty() && tasks.is_empty() {
        let text = "A quiet day: nothing was captured.".to_string();
        crate::lock(&state.store).save_recap(&day, &text, "[]", crate::capture::now_ms()).map_err(|e| e.to_string())?;
        on_token(&text);
        return Ok((text, vec![]));
    }
    let model = crate::lock(&state.settings).model.clone();
    let Backend::Bundled { port } = ensure_backend(state, &model)? else { return Err("Today needs the bundled model runtime".into()) };
    let now = crate::capture::now_ms();
    let mut context = String::new();
    let mut sources = Vec::new();
    for c in cards.iter().rev() {
        let n = sources.len() + 1;
        context.push_str(&format!(
            "[{n}] {} · {} · {} min\nTitle: {}\nSummary: {}\nPeople: {} · Organizations: {} · Projects: {} · Numbers: {}{}\n\n",
            relative_time(c.started_at, now), c.app_name, (c.total_ms / 60_000).max(1), c.title, c.summary,
            c.people.join(", "), c.organizations.join(", "), c.projects.join(", "), c.numbers.join(", "),
            if c.decisions.is_empty() { String::new() } else { format!("\nDecided: {}", c.decisions.join("; ")) }
        ));
        sources.push(AskSource { n, card: Some(c.clone()), file: None, task: None, entity: None });
    }
    if !tasks.is_empty() {
        context.push_str("OPEN COMMITMENTS FROM THAT DAY:\n");
        for t in &tasks {
            context.push_str(&format!("- {} (from \"{}\")\n", t.text, t.title));
        }
        context.push('\n');
    }
    if !apps.is_empty() {
        context.push_str(&format!("TIME BY APP: {}\n\n", apps.iter().map(|(a, m)| format!("{a} {m} min")).collect::<Vec<_>>().join(", ")));
    }
    let who = user_name().map(|n| format!(" The person is {n}.")).unwrap_or_default();
    let insights = crate::lock(&state.store).insights(now).unwrap_or_default();
    if !insights.is_empty() {
        context.push_str("OBSERVATIONS (computed from the whole memory, use the ones that matter, keep the numbers exact):\n");
        for i in &insights {
            context.push_str(&format!("- {i}\n"));
        }
        context.push('\n');
    }
    let user = format!("Day: {} ({}). Now: {}.{who}\n\nMEMORIES:\n{context}Write the briefing.", day, weekday_name(start), format_time(now));
    state.ask_active.store(true, Ordering::Relaxed);
    touch_models();
    let t0 = std::time::Instant::now();
    let text = runtime::chat_stream(port, RECAP_SYSTEM, &user, 450, &mut on_token);
    state.ask_active.store(false, Ordering::Relaxed);
    let took = t0.elapsed().as_secs();
    let text = strip_markdown(&strip_bad_citations(&text?, sources.len()));
    let cited: std::collections::BTreeSet<usize> = citations(&text).into_iter().collect();
    let sources: Vec<AskSource> = sources.into_iter().filter(|s| cited.is_empty() || cited.contains(&s.n)).collect();
    crate::lock(&state.store).save_recap(&day, &text, &serde_json::to_string(&sources).unwrap_or_default(), now).map_err(|e| e.to_string())?;
    log::info!("recap: generated for {day}, {} sources, {took} s", sources.len());
    Ok((text, sources))
}

/// Write a Markdown file into the user's export folder (an Obsidian vault,
/// say) under Lane/<section>/. Silent no-op when the folder is unset.
pub fn export_markdown(state: &AppState, section: &str, name: &str, body: &str) {
    let folder = crate::lock(&state.settings).markdown_folder.trim().to_string();
    if folder.is_empty() {
        return;
    }
    let dir = std::path::PathBuf::from(folder).join("Lane").join(section);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("markdown export: {e}");
        return;
    }
    let safe: String = name.chars().map(|c| if c == '/' || c == ':' || c == '\\' { '-' } else { c }).collect();
    let path = dir.join(format!("{}.md", safe.trim()));
    if let Err(e) = std::fs::write(&path, body) {
        log::warn!("markdown export: {e}");
    }
}

/// A day or a week. Weekly keys are "week-<monday>".
pub fn recap_span(state: &AppState, ms: i64, span: &str, force: bool, on_token: impl FnMut(&str)) -> Result<(String, Vec<AskSource>), String> {
    if span == "day" {
        let r = recap(state, ms, force, on_token)?;
        export_markdown(state, "Daily", &day_of(ms), &format!("# {}\n\n{}\n", day_of(ms), r.0));
        return Ok(r);
    }
    let (day_start, _) = day_bounds(ms);
    if span == "month" {
        let start = month_start(day_start);
        let (y, m, _) = ymd(start);
        let end = if m == 12 { ms_of(y + 1, 1, 1) } else { ms_of(y, m + 1, 1) };
        let key = format!("month-{}", &day_of(start)[..7]);
        let r = recap_range(state, &key, start, end, force, on_token)?;
        export_markdown(state, "Monthly", &day_of(start)[..7], &format!("# {}\n\n{}\n", &day_of(start)[..7], r.0));
        return Ok(r);
    }
    let weekday = (day_start / 1000 + local_offset_secs()).div_euclid(86_400).rem_euclid(7); // 0 = Thursday
    let since_monday = (weekday + 3) % 7;
    let start = day_start - since_monday * 86_400_000;
    let end = start + 7 * 86_400_000;
    let key = format!("week-{}", day_of(start));
    recap_range(state, &key, start, end, force, on_token)
}

fn recap_range(state: &AppState, key: &str, start: i64, end: i64, force: bool, mut on_token: impl FnMut(&str)) -> Result<(String, Vec<AskSource>), String> {
    if !force {
        if let Some((text, _, sources)) = crate::lock(&state.store).recap(key).map_err(|e| e.to_string())? {
            on_token(&text);
            return Ok((text, serde_json::from_str(&sources).unwrap_or_default()));
        }
    }
    let (cards, tasks, decisions) = {
        let store = crate::lock(&state.store);
        (
            store.memories_between(start, end, 24).map_err(|e| e.to_string())?,
            store.tasks_between(start, end).map_err(|e| e.to_string())?,
            store.decisions_between(start, end).map_err(|e| e.to_string())?,
        )
    };
    if cards.is_empty() {
        let text = "A quiet week: nothing was captured.".to_string();
        on_token(&text);
        return Ok((text, vec![]));
    }
    let model = crate::lock(&state.settings).model.clone();
    let Backend::Bundled { port } = ensure_backend(state, &model)? else { return Err("needs the bundled model runtime".into()) };
    let now = crate::capture::now_ms();
    let mut context = String::new();
    let mut sources = Vec::new();
    for c in cards.iter().rev() {
        let n = sources.len() + 1;
        context.push_str(&format!("[{n}] {} · {} · {}\n{}\n\n", relative_time(c.started_at, now), c.app_name, c.title, c.summary));
        sources.push(AskSource { n, card: Some(c.clone()), file: None, task: None, entity: None });
    }
    if !decisions.is_empty() {
        context.push_str("DECISIONS:\n");
        for (d, t, _, _) in &decisions {
            context.push_str(&format!("- {d} (in \"{t}\")\n"));
        }
        context.push('\n');
    }
    if !tasks.is_empty() {
        context.push_str("STILL OPEN:\n");
        for t in &tasks {
            context.push_str(&format!("- {}\n", t.text));
        }
        context.push('\n');
    }
    let who = user_name().map(|n| format!(" The person is {n}.")).unwrap_or_default();
    let month = end - start > 8 * 86_400_000;
    let user = if month {
        format!("Month starting {}. Now: {}.{who}\n\nMEMORIES:\n{context}Write the month in review: the two or three threads that ran through the month, what got decided, the numbers that mattered, what is still open, and who mattered. Speak of the month, not of a day. Five short paragraphs at most.", day_of(start), format_time(now))
    } else {
        format!("Week starting {}. Now: {}.{who}\n\nMEMORIES:\n{context}Write the weekly review: the threads that ran through the week, what got decided, what is still open, and who mattered. Five short paragraphs at most.", day_of(start), format_time(now))
    };
    state.ask_active.store(true, Ordering::Relaxed);
    touch_models();
    let text = runtime::chat_stream(port, RECAP_SYSTEM, &user, 600, &mut on_token);
    state.ask_active.store(false, Ordering::Relaxed);
    let text = strip_markdown(&strip_bad_citations(&text?, sources.len()));
    let cited: std::collections::BTreeSet<usize> = citations(&text).into_iter().collect();
    let sources: Vec<AskSource> = sources.into_iter().filter(|s| cited.is_empty() || cited.contains(&s.n)).collect();
    crate::lock(&state.store).save_recap(key, &text, &serde_json::to_string(&sources).unwrap_or_default(), now).map_err(|e| e.to_string())?;
    export_markdown(state, "Weekly", key, &format!("# Week of {}\n\n{}\n", day_of(start), text));
    Ok((text, sources))
}

fn weekday_name(ms: i64) -> &'static str {
    let days = (ms / 1000 + local_offset_secs()).div_euclid(86_400);
    ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"][days.rem_euclid(7) as usize]
}

pub fn citations(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('[') {
        rest = &rest[i + 1..];
        if let Some(j) = rest.find(']') {
            if let Ok(n) = rest[..j].trim().parse::<usize>() {
                out.push(n);
            }
        }
    }
    out
}

pub fn strip_bad_citations(text: &str, max: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(j) = rest.find(']') {
            let inner = &rest[1..j];
            match inner.trim().parse::<usize>() {
                Ok(n) if n >= 1 && n <= max => out.push_str(&rest[..=j]),
                Ok(_) => {}
                Err(_) => out.push_str(&rest[..=j]),
            }
            rest = &rest[j + 1..];
        } else {
            out.push_str(rest);
            rest = "";
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
mod ask_tests {
    use super::*;

    #[test]
    fn day_bounds_contain_their_timestamp() {
        let ms = 1_789_700_000_000;
        let (a, b) = day_bounds(ms);
        assert!(a <= ms && ms < b && b - a == 86_400_000);
        assert_eq!(day_of(ms), format_time(ms)[..10]);
        assert_eq!(day_of(a), day_of(ms));
        assert_ne!(day_of(a - 1), day_of(ms));
    }

    #[test]
    fn markdown_is_stripped() {
        assert_eq!(strip_markdown("It is *titled* **Bamboo** at `~/x` [1]"), "It is titled Bamboo at ~/x [1]");
    }

    #[test]
    fn citations_are_parsed_and_bad_ones_removed() {
        assert_eq!(citations("Budget ₹35L [1], deadline [3]. See [x]."), vec![1, 3]);
        assert_eq!(strip_bad_citations("A [1] B [9] C [note]", 3), "A [1] B  C [note]");
    }

    #[test]
    fn user_name_is_readable() {
        assert!(user_name().map_or(true, |n| !n.trim().is_empty()));
    }

    #[test]
    fn time_formatting_is_plausible() {
        let s = format_time(1_789_660_182_000);
        assert!(s.starts_with("2026-09-1"), "{s}");
        let now = 1_789_700_000_000;
        assert!(relative_time(now - 3_600_000, now).starts_with("today "));
        assert!(relative_time(now - 86_400_000, now).starts_with("yesterday "));
        let three = relative_time(now - 3 * 86_400_000, now);
        assert!(three.contains(" Sep 2026 "), "{three}");
        assert_eq!(relative_time(now - 40 * 86_400_000, now).matches(' ').count(), 2);
        assert_eq!(short_date("2026-09-06"), "6 Sep 2026");
    }
}

#[cfg(test)]
mod live {
    //! End-to-end over a COPY of a real database, with the app closed:
    //!   RAT_DB="$HOME/Library/Application Support/so.lane.app/copy.db" RAT_Q="What was the Vatsalya budget?" \
    //!   cargo test live_ask -- --ignored --nocapture
    //! The copy must sit next to the real `models/` folder.
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;

    #[test]
    #[ignore]
    fn live_ask() {
        let db = std::path::PathBuf::from(std::env::var("RAT_DB").expect("RAT_DB"));
        let question = std::env::var("RAT_Q").unwrap_or_else(|_| "What did I work on?".into());
        let store = crate::store::Store::open_plain(&db).unwrap_or_else(|_| crate::store::Store::open(&db).unwrap());
        let settings = store.settings();
        let state = crate::AppState {
            store: Mutex::new(store),
            engine: Mutex::new(super::EngineStatus::default()),
            engine_wake: AtomicBool::new(false),
            stale_grant: AtomicBool::new(false),
            ask_active: AtomicBool::new(false),
            blocked: AtomicBool::new(false),
            last_seen: Mutex::new(None),
            rescan_files: std::sync::Arc::new(AtomicBool::new(false)),
            file_queue: Mutex::new(std::collections::VecDeque::new()),
            file_watcher: Mutex::new(None),
            recording: std::sync::Arc::new(Mutex::new(crate::meetings::RecordingStatus::default())),
            settings: Mutex::new(settings),
            paused: AtomicBool::new(false),
            status: Mutex::new(crate::CaptureStatus::default()),
            db_path: db,
            resource_dir: None,
            pause_item: Mutex::new(None),
            meeting_item: Mutex::new(None),
            screen: Mutex::new(None),
        };
        let t = std::time::Instant::now();
        let n = super::embed_pending(&state, 200).unwrap();
        eprintln!("indexed {n} memories in {:.1}s", t.elapsed().as_secs_f64());
        let t = std::time::Instant::now();
        let mut first = None;
        let history: Vec<super::AskTurn> = std::env::var("RAT_PREV_Q")
            .ok()
            .map(|q| vec![super::AskTurn { question: q, answer: std::env::var("RAT_PREV_A").unwrap_or_default() }])
            .unwrap_or_default();
        let (answer, sources) = super::answer_with(&state, &question, &history, false, |_| {
            first.get_or_insert(t.elapsed().as_secs_f64());
        })
        .unwrap();
        eprintln!("\nQ: {question}\nA: {answer}\n");
        for s in &sources {
            match (&s.card, &s.file) {
                (Some(c), _) => eprintln!("[{}] {} · {} · {}", s.n, c.title, c.app_name, c.url.clone().unwrap_or_default()),
                (_, Some(f)) => eprintln!("[{}] FILE {} · {}", s.n, f.name, f.path),
                _ => eprintln!("[{}] {}", s.n, s.task.as_ref().map(|t| format!("TASK {}", t.text)).or_else(|| s.entity.as_ref().map(|e| format!("ENTITY {}", e.name))).unwrap_or_default()),
            }
        }
        eprintln!("first token {:.1}s, total {:.1}s", first.unwrap_or(0.0), t.elapsed().as_secs_f64());
        crate::runtime::shutdown();
    }
}

// ── Meetings ─────────────────────────────────────────────────────────────

/// Is everything in place to record and transcribe? (helper, whisper, model)
pub fn meeting_readiness(state: &AppState) -> (bool, String) {
    let res = state.resource_dir.as_deref();
    if crate::meetings::helper_path(res).is_none() {
        return (false, "The recording helper is missing from this build".into());
    }
    if crate::meetings::whisper_path(res).is_none() {
        return (false, "The speech engine (whisper.cpp) is not installed".into());
    }
    let model = state.db_path.with_file_name("models").join(crate::meetings::whisper_model().file);
    if !model.is_file() {
        return (true, format!("The speech model ({} MB) downloads on first use", crate::meetings::whisper_model().bytes / 1_000_000));
    }
    (true, "ready".into())
}

pub fn start_meeting(app: &AppHandle, state: &AppState, title: String, voice_note: bool) -> Result<i64, String> {
    if crate::lock(&crate::meetings::RECORDER).is_some() {
        return Err("Already recording".into());
    }
    let (ready, why) = meeting_readiness(state);
    if !ready {
        return Err(why);
    }
    let helper = crate::meetings::helper_path(state.resource_dir.as_deref()).ok_or("recorder missing")?;
    let now = crate::capture::now_ms();
    let title = if !title.trim().is_empty() {
        title.trim().to_string()
    } else if voice_note {
        format!("Voice note {}", &format_time(now)[..16])
    } else if let Some(t) = current_event_title(state, now) {
        t
    } else {
        format!("Meeting {}", &format_time(now)[..16])
    };
    let dir = state.db_path.with_file_name("recordings").join(now.to_string());
    let (meeting_id, _activity) = crate::lock(&state.store).create_meeting(&title, now, &dir.display().to_string()).map_err(|e| e.to_string())?;
    {
        let mut st = crate::lock(&state.recording);
        *st = crate::meetings::RecordingStatus { recording: true, meeting_id: Some(meeting_id), started_at: Some(now), ..Default::default() };
    }
    crate::meetings::start(&helper, &dir, meeting_id, now, state.recording.clone(), voice_note)?;
    // Live notes need the speech engine and model; start them if present.
    let res = state.resource_dir.as_deref();
    let model = state.db_path.with_file_name("models").join(crate::meetings::whisper_model().file);
    if let (Some(whisper), true) = (crate::meetings::whisper_path(res), model.is_file()) {
        let lang = crate::lock(&state.settings).meeting_language.clone();
        crate::meetings::spawn_live(whisper, model, dir.clone(), if lang.is_empty() { "auto".into() } else { lang }, state.recording.clone());
    }
    log::info!("meeting {meeting_id}: recording started");
    set_meeting_item(state, true);
    let _ = app.emit("meetings-changed", ());
    Ok(meeting_id)
}

/// Stop recording and transcribe on a background thread.
/// The menu bar item says what pressing it will do, whoever started the
/// recording (tray, notch, meetings page or a call Lane offered to record).
pub fn set_meeting_item(state: &AppState, recording: bool) {
    if let Some(item) = crate::lock(&state.meeting_item).clone() {
        let _ = item.set_text(if recording { "Stop recording ●" } else { "Record meeting" });
    }
}

pub fn stop_meeting(app: AppHandle, state: Arc<AppState>) {
    // A dictation shares the recorder but has no meeting row (id 0). Stopping
    // it as if it were a meeting used to write into a row that does not exist
    // and lose the audio, so hand it back to dictation instead.
    if crate::dictation::active() {
        match crate::dictation::toggle(&app, &state) {
            Ok(_) => log::info!("stop: a dictation was running, finished it instead"),
            Err(e) => log::warn!("stop: dictation: {e}"),
        }
        return;
    }
    let Some((dir, meeting_id, started_at)) = crate::meetings::stop() else { return };
    if meeting_id == 0 {
        log::warn!("stop: recorder had no meeting attached; nothing to save");
        let mut st = crate::lock(&state.recording);
        *st = crate::meetings::RecordingStatus::default();
        let _ = app.emit("meetings-changed", ());
        return;
    }
    let ended = crate::capture::now_ms();
    {
        let mut st = crate::lock(&state.recording);
        st.recording = false;
        st.detail = "transcribing".into();
    }
    set_meeting_item(&state, false);
    let _ = crate::lock(&state.store).set_meeting_status(meeting_id, "transcribing", "", Some(ended));
    let _ = crate::lock(&state.store).extend_activity_of_meeting(meeting_id, ended);
    let _ = app.emit("meetings-changed", ());
    std::thread::Builder::new()
        .name("transcribe".into())
        .spawn(move || {
            // What the meeting app showed while recording: participant
            // names, the chat, shared slides. Kept with the transcript so the
            // memory names the people who were there.
            let (screen, attendees) = take_meeting_screen();
            if !screen.is_empty() {
                if let Ok(activity) = crate::lock(&state.store).meeting_activity(meeting_id) {
                    let _ = crate::lock(&state.store).add_snapshot(activity, &format!("On screen during the meeting:\n{screen}"), started_at);
                }
            }
            let result = transcribe_meeting(&state, &dir, meeting_id, started_at);
            match result {
                Ok(n) => {
                    log::info!("meeting {meeting_id}: transcribed, {n} turns");
                    let _ = crate::lock(&state.store).set_meeting_status(meeting_id, "done", &format!("{n} turns"), None);
                    let _ = crate::lock(&state.store).set_meeting_notes(meeting_id, "", &attendees);
                    match meeting_notes(&state, meeting_id) {
                        Ok(_) => log::info!("meeting {meeting_id}: notes written"),
                        Err(e) => log::warn!("meeting {meeting_id}: notes: {e}"),
                    }
                    let _ = app.emit("meetings-changed", ());
                    if let Ok(md) = meeting_summary(&state, meeting_id, true) {
                        let title = crate::lock(&state.store).list_meetings(500).ok().and_then(|ms| ms.into_iter().find(|m| m.id == meeting_id)).map(|m| m.title).unwrap_or_default();
                        export_markdown(&state, "Meetings", &format!("{} {}", &format_time(started_at)[..16].replace(':', "."), title), &md);
                    }
                }
                Err(e) => {
                    log::warn!("meeting {meeting_id}: {e}");
                    let _ = crate::lock(&state.store).set_meeting_status(meeting_id, "failed", &e, None);
                }
            }
            crate::lock(&state.recording).detail = String::new();
            state.engine_wake.store(true, Ordering::Relaxed);
            let _ = app.emit("meetings-changed", ());
        })
        .expect("spawn transcribe");
}

fn transcribe_meeting(state: &AppState, dir: &std::path::Path, meeting_id: i64, started_at: i64) -> Result<usize, String> {
    let res = state.resource_dir.as_deref();
    let whisper = crate::meetings::whisper_path(res).ok_or("speech engine missing")?;
    let models_dir = state.db_path.with_file_name("models");
    let model = models_dir.join(crate::meetings::whisper_model().file);
    if !model.is_file() {
        let _ = crate::lock(&state.store).set_meeting_status(meeting_id, "transcribing", "downloading speech model", None);
        let progress = runtime::DownloadProgress::new();
        runtime::download(crate::meetings::whisper_model(), &model, &progress)?;
    }
    let (lang, keep) = {
        let s = crate::lock(&state.settings);
        (s.meeting_language.clone(), s.keep_audio)
    };
    let lang = if lang.is_empty() { "auto".to_string() } else { lang };
    let mic = crate::meetings::transcribe(&whisper, &model, &dir.join("mic.wav"), &lang)?;
    let system = crate::meetings::transcribe(&whisper, &model, &dir.join("system.wav"), &lang)?;
    // Voices on the other side, when the user asked for it.
    let data_dir = state.db_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let diar = if crate::lock(&state.settings).diarize_enabled && !system.is_empty() {
        let _ = crate::lock(&state.store).set_meeting_status(meeting_id, "transcribing", "telling speakers apart", None);
        let prepared = crate::diarize::ensure(&data_dir, |msg| {
            let _ = crate::lock(&state.store).set_meeting_status(meeting_id, "transcribing", msg, None);
        });
        match prepared.and_then(|_| crate::diarize::diarize(&data_dir, &dir.join("system.wav"))) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("meeting {meeting_id}: speakers: {e}");
                vec![]
            }
        }
    } else {
        vec![]
    };
    let (turns, labels) = crate::meetings::merge_with_speakers(mic, system, &diar);
    if !labels.is_empty() {
        let speakers: Vec<crate::store::Speaker> = labels.iter().map(|l| crate::store::Speaker { label: l.clone(), name: String::new() }).collect();
        let _ = crate::lock(&state.store).set_meeting_speakers(meeting_id, &speakers);
    }
    let text = crate::meetings::render(&turns);
    let activity: i64 = crate::lock(&state.store).meeting_activity(meeting_id).map_err(|e| e.to_string())?;
    {
        let store = crate::lock(&state.store);
        for (i, chunk) in crate::meetings::chunks(&text, 3_000).iter().enumerate() {
            let at = started_at + (i as i64) * 60_000;
            store.add_snapshot(activity, chunk, at).map_err(|e| e.to_string())?;
        }
    }
    if !keep {
        let _ = std::fs::remove_dir_all(dir);
    }
    if turns.is_empty() {
        return Err("nothing was heard".into());
    }
    Ok(turns.len())
}

/// Notes like a good assistant would write straight after the call: what it
/// was about, the points made, what was decided, who owes what, what is
/// still open. Built from the transcript on this Mac and kept with the
/// meeting; the memory card and the share sheet use them.
pub fn meeting_notes(state: &AppState, meeting_id: i64) -> Result<String, String> {
    let (activity, title, attendees) = {
        let store = crate::lock(&state.store);
        let m = store.list_meetings(500).map_err(|e| e.to_string())?.into_iter().find(|m| m.id == meeting_id).ok_or("no such meeting")?;
        (m.activity_id, m.title, m.attendees)
    };
    let transcript = crate::lock(&state.store).activity_text(activity, 60_000).map_err(|e| e.to_string())?;
    if transcript.chars().count() < 80 {
        return Err("too short for notes".into());
    }
    // Long calls: the first and last stretches carry the agenda and the
    // wrap-up; the middle is sampled so the whole call is represented.
    let body: String = if transcript.chars().count() <= 14_000 {
        transcript
    } else {
        let chars: Vec<char> = transcript.chars().collect();
        let n = chars.len();
        let head: String = chars[..5_000].iter().collect();
        let mid: String = chars[n / 2 - 2_000..n / 2 + 2_000].iter().collect();
        let tail: String = chars[n - 5_000..].iter().collect();
        format!("{head}\n[…]\n{mid}\n[…]\n{tail}")
    };
    let model = crate::lock(&state.settings).model.clone();
    let Backend::Bundled { port } = ensure_backend(state, &model)? else { return Err("needs the bundled model runtime".into()) };
    let schema = json!({"type": "object", "properties": {
        "summary": {"type": "string", "maxLength": 700},
        "key_points": {"type": "array", "maxItems": 6, "items": {"type": "string", "maxLength": 160}},
        "decisions": {"type": "array", "maxItems": 5, "items": {"type": "string", "maxLength": 160}},
        "action_items": {"type": "array", "maxItems": 8, "items": {"type": "object", "properties": {"who": {"type": "string", "maxLength": 40}, "what": {"type": "string", "maxLength": 160}, "when": {"type": "string", "maxLength": 40}}, "required": ["who", "what", "when"]}},
        "open_questions": {"type": "array", "maxItems": 4, "items": {"type": "string", "maxLength": 160}}
    }, "required": ["summary", "key_points", "decisions", "action_items", "open_questions"]});
    let who = user_name().map(|n| format!(" 'You' in the transcript is {n}.")).unwrap_or_default();
    let system = format!("You write meeting notes from a transcript. 'You' is the user, 'Others' the other side.{who} Only what was said: no invention, no filler. Summary: three to five sentences on what the meeting was about and where it landed. Key points: the substantive things said. Decisions: only things actually agreed. Action items: who owes what by when ('' when no date was said). Open questions: raised and not settled. Use the transcript's own names, figures and dates.");
    let names = if attendees.is_empty() { String::new() } else { format!("PEOPLE ON THE CALL: {}\n\n", attendees.join(", ")) };
    state.ask_active.store(true, Ordering::Relaxed);
    touch_models();
    let result = runtime::chat_json(port, &system, &format!("MEETING: {title}\n{names}TRANSCRIPT:\n{body}"), schema, 900);
    state.ask_active.store(false, Ordering::Relaxed);
    let (raw, _) = result?;
    let v: Value = serde_json::from_str(&repair_json(&raw)).map_err(|e| format!("notes were not readable: {e}"))?;
    let list = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).map(str::trim).filter(|x| x.chars().count() > 3).map(String::from).collect::<Vec<_>>()).unwrap_or_default();
    let mut md = String::new();
    if let Some(sum) = v["summary"].as_str().map(str::trim).filter(|t| !t.is_empty()) {
        md.push_str(sum);
        md.push_str("\n\n");
    }
    for (heading, items) in [("Key points", list("key_points")), ("Decisions", list("decisions")), ("Open questions", list("open_questions"))] {
        if !items.is_empty() {
            md.push_str(&format!("**{heading}**\n"));
            for it in items {
                md.push_str(&format!("- {it}\n"));
            }
            md.push('\n');
        }
    }
    let actions: Vec<String> = v["action_items"].as_array().map(|a| a.iter().filter_map(|x| {
        let what = x["what"].as_str()?.trim();
        if what.chars().count() < 4 { return None; }
        let who = x["who"].as_str().unwrap_or("").trim();
        let when = x["when"].as_str().unwrap_or("").trim();
        let when = when.strip_prefix("by ").or_else(|| when.strip_prefix("By ")).unwrap_or(when).trim();
        // "Sarah: Sarah to book…" reads badly; the name is dropped when the
        // line already starts with it.
        let owner = if who.is_empty() || what.to_lowercase().starts_with(&who.to_lowercase()) { String::new() } else { format!("{who}: ") };
        Some(format!("{owner}{what}{}", if when.is_empty() || what.to_lowercase().contains(&when.to_lowercase()) { String::new() } else { format!(" (by {when})") }))
    }).collect()).unwrap_or_default();
    if !actions.is_empty() {
        md.push_str("**Action items**\n");
        for a in actions {
            md.push_str(&format!("- [ ] {a}\n"));
        }
        md.push('\n');
    }
    let md = md.trim().to_string();
    if md.is_empty() {
        return Err("nothing to note".into());
    }
    crate::lock(&state.store).set_meeting_notes(meeting_id, &md, &attendees).map_err(|e| e.to_string())?;
    Ok(md)
}

/// (last check ms, last nudge ms, title nudged, seconds without the call window, auto-started)
static CALL_STATE: std::sync::Mutex<(i64, i64, String, i64, bool)> = std::sync::Mutex::new((0, 0, String::new(), 0, false));

/// A call is on when a meeting app or site is the front window. Every 20 s:
/// with auto-listen on, recording starts by itself (named after the
/// calendar event) and stops two minutes after the call window has gone;
/// off, the notch says "In a call?" once per call so one click starts it.
/// Live help and notes then work like any recorded meeting.

/// A recording nobody speaks into is a recording nobody wanted. If six
/// minutes pass with no sound at all, stop and keep what there is.
const SILENCE_STOP_MS: i64 = 6 * 60 * 1000;

fn silence_step(app: &AppHandle, state: &Arc<AppState>) {
    let (recording, started_at, last_sound) = {
        let st = crate::lock(&state.recording);
        (st.recording, st.started_at, st.last_sound_at)
    };
    if !recording || crate::dictation::active() {
        return;
    }
    let now = crate::capture::now_ms();
    let quiet_since = last_sound.or(started_at).unwrap_or(now);
    if now - quiet_since < SILENCE_STOP_MS {
        return;
    }
    log::info!("meeting: six minutes without a sound, stopping");
    notch(app, "help", "Recording stopped", vec!["Six minutes went by without a sound, so Lane stopped recording. Whatever it heard before that is kept.".into()]);
    stop_meeting(app.clone(), Arc::clone(state));
}

fn call_detect_step(app: &AppHandle, state: &Arc<AppState>) {
    let now = crate::capture::now_ms();
    {
        let mut st = crate::lock(&CALL_STATE);
        if now - st.0 < 20_000 {
            return;
        }
        st.0 = now;
    }
    if crate::dictation::active() {
        return;
    }
    let (auto, read_web) = {
        let s = crate::lock(&state.settings);
        (s.auto_listen_calls, s.read_browser_text)
    };
    let recording = crate::lock(&state.recording).recording;
    let front = crate::capture::platform::observe(read_web, false);
    let in_call = front.as_ref().map_or(false, |o| is_meeting_window(&o.app_name, o.url.as_deref()));
    let title = front.as_ref().map(|o| o.window_title.clone()).unwrap_or_default();
    if in_call {
        log::info!("call: {} in front ({})", front.as_ref().map(|o| o.app_name.as_str()).unwrap_or("?"), front.as_ref().and_then(|o| o.url.as_deref()).unwrap_or(""));
    }
    let mut st = crate::lock(&CALL_STATE);
    if recording {
        if st.4 {
            st.3 = if in_call { 0 } else { st.3 + 20 };
            if st.3 >= 120 {
                st.4 = false;
                st.3 = 0;
                drop(st);
                log::info!("call: window gone for two minutes, stopping");
                stop_meeting(app.clone(), Arc::clone(state));
                notch(app, "help", "Call ended", vec!["Writing the notes now.".into()]);
            }
        }
        return;
    }
    if !in_call {
        return;
    }
    if auto {
        drop(st);
        match start_meeting(app, state, String::new(), false) {
            Ok(id) => {
                let mut st = crate::lock(&CALL_STATE);
                st.4 = true;
                st.3 = 0;
                log::info!("call: auto-started meeting {id}");
                notch(app, "help", "In a call", vec!["Listening. Live help appears here; notes and a memory when it ends.".into()]);
            }
            Err(e) => log::warn!("call: could not start: {e}"),
        }
    } else if st.2 != title || now - st.1 > 15 * 60_000 {
        st.1 = now;
        st.2 = title;
        drop(st);
        notch(app, "reminder", "In a call?", vec!["Press Record for live help now and notes when it ends.".into(), "Settings → Meetings can start this by itself.".into()]);
    }
}

/// Text seen in the meeting app while a recording runs, and the names on it.
static MEETING_SCREEN: std::sync::Mutex<(Vec<String>, i64)> = std::sync::Mutex::new((Vec::new(), 0));

const MEETING_APPS: &[&str] = &["zoom.us", "zoom", "Microsoft Teams", "Teams", "Webex", "FaceTime", "Slack", "Google Meet", "Around", "Whereby"];
const MEETING_SITES: &[&str] = &["meet.google.com", "zoom.us", "teams.microsoft.com", "teams.live.com", "whereby.com", "webex.com", "app.slack.com/huddle"];

fn is_meeting_window(app_name: &str, url: Option<&str>) -> bool {
    MEETING_APPS.iter().any(|a| a.eq_ignore_ascii_case(app_name)) || url.map_or(false, |u| MEETING_SITES.iter().any(|s| u.contains(s)))
}

/// While a meeting is recorded: every 45 s, if the front window is the
/// meeting app or site, keep its visible text (participant list, chat,
/// slide titles). Only then: meeting apps are otherwise excluded from
/// capture, and the user pressed Record for this call.
fn meeting_screen_step(state: &AppState) -> bool {
    if !crate::lock(&state.recording).recording || crate::dictation::active() {
        return false;
    }
    let now = crate::capture::now_ms();
    {
        let guard = crate::lock(&MEETING_SCREEN);
        if now - guard.1 < 45_000 {
            return false;
        }
    }
    crate::lock(&MEETING_SCREEN).1 = now;
    let read_web = crate::lock(&state.settings).read_browser_text;
    let Some(o) = crate::capture::platform::observe(read_web, true) else { return false };
    if !is_meeting_window(&o.app_name, o.url.as_deref()) {
        return false;
    }
    let text = o.text.map(|t| crate::clean::clean_text(&t, |_| 0)).unwrap_or_default();
    let mut guard = crate::lock(&MEETING_SCREEN);
    let mut added = 0;
    for line in std::iter::once(o.window_title.as_str()).chain(text.lines()) {
        let l = line.trim();
        if l.chars().count() < 2 || guard.0.iter().any(|x| x == l) {
            continue;
        }
        guard.0.push(l.chars().take(200).collect());
        added += 1;
        if guard.0.len() >= 400 {
            break;
        }
    }
    added > 0
}

/// Two to four capitalised words, no digits: what a participant list looks like.
pub fn looks_like_name(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    if !(2..=4).contains(&words.len()) || line.chars().count() > 40 || line.chars().any(|c| c.is_ascii_digit() || "@/:#()[]".contains(c)) {
        return false;
    }
    let stop = ["You", "Meeting", "Chat", "Mute", "Unmute", "Leave", "Share", "Screen", "Camera", "Participants", "More", "Record", "Recording", "Host", "Guest", "Join", "Now", "Everyone", "Raise", "Hand"];
    words.iter().all(|w| w.chars().next().map_or(false, |c| c.is_uppercase()) && w.chars().all(|c| c.is_alphabetic() || c == '\'' || c == '-' || c == '.'))
        && !words.iter().any(|w| stop.contains(w))
}

/// Everything seen, and the names in it, then reset for the next call.
fn take_meeting_screen() -> (String, Vec<String>) {
    let lines = std::mem::take(&mut crate::lock(&MEETING_SCREEN).0);
    let mut names: Vec<String> = Vec::new();
    for l in &lines {
        let l = l.trim_end_matches(" (You)").trim_end_matches(" (Host)").trim_end_matches(" (Guest)").trim();
        if looks_like_name(l) && !names.iter().any(|n| n.eq_ignore_ascii_case(l)) {
            names.push(l.to_string());
            if names.len() >= 12 {
                break;
            }
        }
    }
    let text: String = lines.join("\n").chars().take(6_000).collect();
    (text, names)
}

/// A shareable summary of a meeting, built from its memory card and tasks,
/// with the transcript optional. Plain Markdown the user can paste anywhere.
pub fn meeting_summary(state: &AppState, meeting_id: i64, include_transcript: bool) -> Result<String, String> {
    let store = crate::lock(&state.store);
    let meetings = store.list_meetings(500).map_err(|e| e.to_string())?;
    let m = meetings.into_iter().find(|m| m.id == meeting_id).ok_or("no such meeting")?;
    let cards = store.entity_memories_for_activity(m.activity_id).map_err(|e| e.to_string())?;
    let card = cards.first();
    let date = format_time(m.started_at);
    let duration = m.ended_at.map(|e| format!(" · {} min", ((e - m.started_at) / 60_000).max(1))).unwrap_or_default();
    let mut out = format!("# {}\n\n{}{}\n\n", card.map(|c| c.title.as_str()).unwrap_or(&m.title), &date[..16], duration);
    if !m.attendees.is_empty() {
        out.push_str(&format!("**Attendees:** {}\n\n", m.attendees.join(", ")));
    }
    if !m.notes.trim().is_empty() {
        out.push_str(m.notes.trim());
        out.push_str("\n\n");
    }
    if let Some(c) = card {
        if m.notes.trim().is_empty() {
            out.push_str(&format!("{}\n\n", c.summary));
        }
        if !c.people.is_empty() {
            out.push_str(&format!("**People:** {}\n\n", c.people.join(", ")));
        }
        if !c.organizations.is_empty() {
            out.push_str(&format!("**Organisations:** {}\n\n", c.organizations.join(", ")));
        }
        if !c.numbers.is_empty() || !c.dates.is_empty() {
            out.push_str(&format!("**Figures and dates:** {}\n\n", c.numbers.iter().chain(c.dates.iter()).cloned().collect::<Vec<_>>().join(", ")));
        }
    }
    let tasks = store.tasks_for_activity(m.activity_id).map_err(|e| e.to_string())?;
    if !tasks.is_empty() && !m.notes.contains("Action items") {
        out.push_str("**Action items**\n");
        for t in tasks {
            out.push_str(&format!("- [ ] {}\n", t));
        }
        out.push('\n');
    }
    if include_transcript {
        let text = store.activity_text(m.activity_id, 200_000).map_err(|e| e.to_string())?;
        out.push_str("---\n\n**Transcript**\n\n");
        out.push_str(&text);
        out.push_str("\n\n");
    }
    out.push_str("---\nPrepared with Lane, the memory that stays on your Mac · lane.so\n");
    Ok(out)
}

#[cfg(test)]
mod export_live {
    //! RAT_DB=<encrypted db> RAT_OUT=<plain copy> cargo test live_export_plain -- --ignored
    #[test]
    #[ignore]
    fn live_export_plain() {
        let db = std::path::PathBuf::from(std::env::var("RAT_DB").unwrap());
        let out = std::path::PathBuf::from(std::env::var("RAT_OUT").unwrap());
        crate::store::Store::open(&db).unwrap().export_plaintext(&out).unwrap();
        eprintln!("exported {}", out.display());
    }
}

#[cfg(test)]
mod retrieval_terms_tests {
    use super::*;
    #[test]
    fn citations_are_not_figures() {
        let u = unverified_figures("The budget is 40 lakh [1][2], the visit on the 22nd [23][24].", "[1] budget 40 lakh · visit on the 22nd");
        assert!(u.is_empty(), "{u:?}");
        let u = unverified_figures("It cost 75 lakh [1].", "[1] budget 40 lakh");
        assert_eq!(u, vec!["75 lakh"]);
    }
    #[test]
    fn invented_weekdays_are_flagged() {
        let u = unverified_figures("The meeting was on Tuesday at 09:38.", "[1] today 09:38 · Meeting");
        assert!(u.iter().any(|x| x == "Tuesday"));
        let ok = unverified_figures("The meeting was on Tuesday.", "[1] Tue 16 Sep 09:38 · Meeting");
        assert!(ok.is_empty());
    }
    #[test]
    fn calendar_spans_parse() {
        let now = crate::capture::now_ms();
        let (a, b) = date_range("what did i spend on domains this year", now).unwrap();
        assert!(b > now && now - a < 366 * 86_400_000 && now - a > 0);
        let (a2, b2) = date_range("how much last month", now).unwrap();
        assert!(b2 <= now && b2 - a2 >= 28 * 86_400_000 && b2 - a2 <= 31 * 86_400_000);
    }
    #[test]
    fn disqualified_list_lines_go() {
        let t = "a.com (expires 2026/09/08) [1]\nb.com (expires Aug 26, 2026) [2] — note: this expires in August, so does not count\nNo other domains expire this month.";
        let out = drop_disqualified_lines(t);
        assert!(out.contains("a.com") && !out.contains("b.com") && out.contains("No other domains"));
        assert!(!drop_disqualified_lines("x.com [1]\nNote: only September counts.").contains("Note:"));
    }
    #[test]
    fn time_words_go_and_money_words_come() {
        let t = retrieval_terms("What did I spend on domains this year?");
        assert!(!t.contains("year") && !t.contains("this"));
        assert!(t.contains("domains") && t.contains("spending") && t.contains("invoice"));
        assert_eq!(retrieval_terms("Who is Sarah Khan?"), "Who is Sarah Khan");
    }
}

#[cfg(test)]
mod task_filter_tests {
    use super::*;
    #[test]
    fn rephrasings_and_ai_chats() {
        assert!(same_task("Increase font sizes throughout the document for a more substantial appearance", "Increase font sizes throughout for a more substantial look"));
        assert!(!same_task("Send the tender to Sarah by Friday", "Book the site visit on the 22nd"));
        assert!(ai_chat_source("Google Chrome", Some("https://chatgpt.com/c/abc")));
        assert!(!ai_chat_source("Google Chrome", Some("https://mail.google.com")));
    }
    #[test]
    fn articles_only_keep_first_person_tasks() {
        let t = vec!["Register for the Anandi-NEAT 2026".to_string(), "I will post the WhatsApp message tonight".to_string()];
        assert_eq!(tasks_for_kind("article", t.clone()), vec!["I will post the WhatsApp message tonight"]);
        assert_eq!(tasks_for_kind("email", t.clone()).len(), 2);
        assert_eq!(tasks_for_kind("email", (0..9).map(|i| format!("Send report number {i} to Sarah")).collect()).len(), 5);
    }
    #[test]
    fn form_questions_are_not_commitments() {
        assert!(looks_like_form_question("Have you earlier served as a member for any evaluation of applications for Fund?"));
        assert!(looks_like_form_question("Why do you wish to be considered as a Jury Member"));
        assert!(looks_like_form_question("Please share your experience for the same"));
        assert!(!looks_like_form_question("Send the revised tender to Sarah by Friday"));
        assert!(!looks_like_form_question("Tom to fix auth by Monday"));
        let src = "have you earlier served as a member for any evaluation send the revised tender to sarah by friday";
        let (kept, dropped) = verify_tasks(vec!["Have you earlier served as a member for any evaluation?".into(), "Send the revised tender to Sarah by Friday".into()], src);
        assert_eq!(kept, vec!["Send the revised tender to Sarah by Friday"]);
        assert_eq!(dropped, 1);
    }
}

#[cfg(test)]
mod meeting_screen_tests {
    use super::*;
    #[test]
    fn names_from_a_participant_list() {
        assert!(looks_like_name("Sarah Khan"));
        assert!(looks_like_name("Dr. Amrita Baruah"));
        assert!(!looks_like_name("Mute Camera"));
        assert!(!looks_like_name("Meeting 2026-09-20"));
        assert!(!looks_like_name("sarah khan"));
        assert!(!looks_like_name("Join now"));
        assert!(is_meeting_window("zoom.us", None));
        assert!(is_meeting_window("Google Chrome", Some("https://meet.google.com/abc-defg-hij")));
        assert!(!is_meeting_window("Google Chrome", Some("https://example.com")));
    }
}

#[cfg(test)]
mod intent_tests {
    use super::*;
    #[test]
    fn intents_and_windows() {
        let now = crate::capture::now_ms();
        let (today, tomorrow) = day_bounds(now);
        assert_eq!(classify("Who is Sarah Khan?", now).kind, IntentKind::Person);
        assert_eq!(classify("What happened with the tender?", now).kind, IntentKind::Timeline);
        assert_eq!(classify("List all the vendors we talked to", now).kind, IntentKind::List);
        assert_eq!(classify("What should I follow up on?", now).kind, IntentKind::Actions);
        assert_eq!(classify("When did we last speak to Acme?", now).kind, IntentKind::Temporal);
        assert_eq!(classify("What was the Vatsalya budget?", now).kind, IntentKind::Factual);
        assert_eq!(date_range("what did i do yesterday", now), Some((today - 86_400_000, today)));
        assert_eq!(date_range("what did i read today", now), Some((today, tomorrow)));
        let (a, b) = date_range("meetings in the last 3 days", now).unwrap();
        assert!(b == tomorrow && now - a >= 3 * 86_400_000 - 1);
        let (a, b) = date_range("what came up last week", now).unwrap();
        assert_eq!(b - a, 7 * 86_400_000);
        assert!(b <= today);
        assert!(date_range("what is the budget", now).is_none());
        let (a, b) = date_range("who did i meet on monday", now).unwrap();
        assert_eq!(b - a, 86_400_000);
        assert!(a < today);
    }
}

#[cfg(test)]
mod thought_tests {
    #[test]
    fn repeated_sentences_are_found_once() {
        let t = "We will deliver the booklets by Friday. The budget is fine. We will deliver the booklets by Friday! Short one. Short one.";
        let r = super::repeated_sentences(t);
        assert_eq!(r, vec!["We will deliver the booklets by Friday"]);
    }
}

#[cfg(test)]
mod verify_answer_tests {
    #[test]
    fn figures_absent_from_sources_are_flagged() {
        let ctx = "Budget is ₹35 lakh inclusive of GST. Tender closes 14 October 2026. Sessions 1,240.";
        let ok = super::unverified_figures("The budget was 35 lakh [1], closing on 14 October, with 1240 sessions.", ctx);
        assert!(ok.is_empty(), "{ok:?}");
        let bad = super::unverified_figures("The budget was 37 lakh, closing on 21 October, about 12% up.", ctx);
        assert_eq!(bad, vec!["37 lakh", "21 October", "12%"]);
        assert!(super::unverified_figures("See [12] and [3].", ctx).is_empty(), "citations are not figures");
    }
}

#[cfg(test)]
mod repair_tests {
    use super::repair_json;
    #[test]
    fn truncated_json_is_closed_and_parses() {
        let cases = [
            r#"{"keep": true, "kind": "document", "title": "Budget", "summary": "A long summ"#,
            r#"{"keep": true, "kind": "document", "title": "Budget", "summary": "ok", "people": ["Asha", "Ra"#,
            r#"{"keep": true, "kind": "document", "title": "Budget", "summary": "ok", "people": ["Asha"], "facts": [{"subject": "x", "attribute": "y", "value": "z"}, {"subject": "q"#,
            r#"{"keep": true, "kind": "document", "title": "Budget", "summary": "ok", "people": ["Asha"],"#,
            r#"{"keep": true, "kind": "document", "title": "Budget", "summary": "ok", "dates":"#,
        ];
        for c in cases {
            let fixed = repair_json(c);
            let v: serde_json::Value = serde_json::from_str(&fixed).unwrap_or_else(|e| panic!("{c}\n→ {fixed}\n{e}"));
            assert_eq!(v["title"], "Budget", "{fixed}");
        }
        let v: serde_json::Value = serde_json::from_str(&repair_json(cases[1])).unwrap();
        assert_eq!(v["people"], serde_json::json!(["Asha"]), "the unfinished name is dropped, the finished one kept");
        let v: serde_json::Value = serde_json::from_str(&repair_json(cases[2])).unwrap();
        assert_eq!(v["facts"].as_array().unwrap().len(), 1);
    }
}

#[cfg(test)]
mod fact_tests {
    use crate::store::NewFact;
    #[test]
    fn facts_need_literal_values_and_grounded_subjects() {
        let src = super::normalize("Vatsalya proposal: the budget is ₹35 lakh inclusive of GST. Tender due 14 October.");
        let items = vec![
            NewFact { subject: "Vatsalya proposal".into(), attribute: "Budget".into(), value: "₹35 lakh".into(), owner: "unknown".into(), stance: "stated".into() },
            NewFact { subject: "Tender".into(), attribute: "deadline".into(), value: "14 October".into(), owner: "unknown".into(), stance: "stated".into() },
            NewFact { subject: "Vatsalya proposal".into(), attribute: "budget".into(), value: "35 lakh".into(), owner: "unknown".into(), stance: "stated".into() }, // duplicate key
            NewFact { subject: "Vatsalya proposal".into(), attribute: "approver".into(), value: "Sarah Khan".into(), owner: "unknown".into(), stance: "stated".into() }, // invented
            NewFact { subject: "Moon base".into(), attribute: "cost".into(), value: "₹35 lakh".into(), owner: "unknown".into(), stance: "stated".into() }, // subject not in source
        ];
        let (kept, dropped) = super::verify_facts(items, &src);
        assert_eq!(kept.len(), 2);
        assert_eq!(dropped, 2);
        assert_eq!(kept[0].attribute, "budget");
    }
}

#[cfg(test)]
mod integration_tests {
    #[test]
    fn notion_page_id_from_url_or_id() {
        let want = "0f3a5b6c-7d8e-4f90-a1b2-c3d4e5f60718";
        assert_eq!(super::notion_page_id("https://www.notion.so/team/Meeting-notes-0f3a5b6c7d8e4f90a1b2c3d4e5f60718?v=1").as_deref(), Some(want));
        assert_eq!(super::notion_page_id(want).as_deref(), Some(want));
        assert!(super::notion_page_id("not a page").is_none());
    }
}