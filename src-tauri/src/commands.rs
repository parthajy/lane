//! Tauri commands: the thin layer between the UI and the store, engine and
//! helpers. Every command runs off the main thread (`async`) except the
//! window and dialog ones listed in `lib.rs: hygiene_tests`.

use crate::capture::platform;
use crate::engine::EngineStatus;
use crate::meetings::RecordingStatus;
use crate::permissions::{self, PermissionReport};
use crate::privacy;
use crate::store::{ActivityDetail, ActivitySummary, Entity, FileHit, FileStats, Graph, MemoryCard, MemoryCounts, Meeting, SearchHit, Settings, Stats, Task};
use crate::{AppState, CaptureStatus};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

static ASK_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

// ── Status and permissions ───────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    paused: bool,
    capture: CaptureStatus,
    stats: Stats,
    db_path: String,
    /// First letter of the Mac user's name, for the profile button.
    user_initial: String,
    user_name: Option<String>,
}

#[tauri::command(async)]
pub fn get_status(state: State<'_, Arc<AppState>>, today_start: i64) -> Res<Status> {
    let capture = crate::lock(&state.status).clone();
    let stats = crate::lock(&state.store).stats(today_start).map_err(err)?;
    Ok(Status {
        paused: state.paused.load(Ordering::Relaxed),
        capture,
        stats,
        db_path: state.db_path.display().to_string(),
        user_initial: crate::engine::user_name_with(&state).and_then(|n| n.chars().next()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "L".into()),
        user_name: crate::engine::user_name_with(&state),
    })
}

#[tauri::command(async)]
pub fn set_paused(state: State<'_, Arc<AppState>>, paused: bool) {
    state.set_paused(paused);
}

#[tauri::command(async)]
pub fn get_permissions(state: State<'_, Arc<AppState>>) -> PermissionReport {
    permissions::report(state.stale_grant.load(Ordering::Relaxed))
}

#[tauri::command(async)]
pub fn request_accessibility() -> Res<bool> {
    permissions::request_accessibility()
}

#[tauri::command]
pub fn open_settings_pane(pane: String) -> Res<()> {
    permissions::open_pane(&pane)
}

#[tauri::command(async)]
pub fn reset_accessibility() -> Res<()> {
    permissions::reset_accessibility()
}

#[tauri::command(async)]
pub fn set_launch_at_login(enabled: bool) -> Res<bool> {
    permissions::set_launch_at_login(enabled)
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyLists {
    always_skipped: &'static [&'static str],
    messaging_apps: &'static [&'static str],
    messaging_sites: &'static [&'static str],
    email_apps: &'static [&'static str],
    email_sites: &'static [&'static str],
}

#[tauri::command(async)]
pub fn get_privacy_lists() -> PrivacyLists {
    PrivacyLists {
        always_skipped: privacy::SYSTEM_EXCLUDED_APPS,
        messaging_apps: privacy::MESSAGING_APPS,
        messaging_sites: privacy::MESSAGING_URLS,
        email_apps: privacy::EMAIL_APPS,
        email_sites: privacy::EMAIL_URLS,
    }
}

// ── Activities ───────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn list_activities(state: State<'_, Arc<AppState>>, before: Option<i64>, limit: Option<u32>) -> Res<Vec<ActivitySummary>> {
    crate::lock(&state.store).list_activities(before, limit.unwrap_or(100).min(500)).map_err(err)
}

#[tauri::command(async)]
pub fn get_activity(state: State<'_, Arc<AppState>>, id: i64) -> Res<Option<ActivityDetail>> {
    crate::lock(&state.store).get_activity(id).map_err(err)
}

#[tauri::command(async)]
pub fn search(state: State<'_, Arc<AppState>>, query: String, limit: Option<u32>) -> Res<Vec<SearchHit>> {
    crate::lock(&state.store).search(&query, limit.unwrap_or(50).min(200)).map_err(err)
}

#[tauri::command(async)]
pub fn delete_activity(app: AppHandle, state: State<'_, Arc<AppState>>, id: i64) -> Res<bool> {
    let images = crate::lock(&state.store).images_of(Some(id), None).unwrap_or_default();
    let ok = crate::lock(&state.store).delete_activity(id).map_err(err)?;
    crate::shots::remove_all(&images);
    let _ = app.emit("activity-changed", ());
    let _ = app.emit("memories-changed", ());
    Ok(ok)
}

// ── Settings ─────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn get_settings(state: State<'_, Arc<AppState>>) -> Res<Settings> {
    Ok(crate::lock(&state.settings).clone())
}

/// "7:30" → "07:30"; anything else → "" (off).
fn valid_time(t: &str) -> String {
    let t = t.trim();
    let Some((h, m)) = t.split_once(':') else { return String::new() };
    match (h.parse::<u32>(), m.parse::<u32>()) {
        (Ok(h), Ok(m)) if h < 24 && m < 60 => format!("{h:02}:{m:02}"),
        _ => String::new(),
    }
}

fn tidy(list: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in list {
        let t = item.trim().to_string();
        if !t.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
            out.push(t);
        }
    }
    out
}

#[tauri::command(async)]
pub fn update_settings(app: AppHandle, state: State<'_, Arc<AppState>>, settings: Settings) -> Res<Settings> {
    // One guard, read everything, release: never two locks in one expression.
    let (trusted_build, labels_last_sent_at, last_backup_at, notion_last_sync_at, old_folders, old_index_files, contacts_last_sync_at, old_mail, old_mail_memories, old_contacts, old_notch) = {
        let cur = crate::lock(&state.settings);
        (
            cur.trusted_build.clone(),
            cur.labels_last_sent_at,
            cur.last_backup_at,
            cur.notion_last_sync_at,
            cur.index_folders.clone(),
            cur.index_files,
            cur.contacts_last_sync_at,
            cur.mail_enabled,
            cur.mail_memories,
            cur.contacts_enabled,
            cur.notch_enabled,
        )
    };
    let clean = Settings {
        display_name: settings.display_name.trim().chars().take(40).collect(),
        interval_secs: settings.interval_secs.clamp(2, 60),
        idle_minutes: settings.idle_minutes.clamp(1, 60),
        excluded_apps: tidy(settings.excluded_apps),
        excluded_url_patterns: tidy(settings.excluded_url_patterns),
        read_browser_text: settings.read_browser_text,
        exclude_messaging: settings.exclude_messaging,
        exclude_email: settings.exclude_email,
        onboarding_done: settings.onboarding_done,
        redact_contacts: settings.redact_contacts,
        model: settings.model.trim().to_string(),
        trusted_build,
        index_folders: tidy(settings.index_folders),
        index_files: settings.index_files,
        keep_audio: settings.keep_audio,
        markdown_folder: settings.markdown_folder.trim().to_string(),
        contribute_labels: settings.contribute_labels,
        tester_token: settings.tester_token.trim().to_string(),
        labels_last_sent_at,
        backup_enabled: settings.backup_enabled,
        backup_folder: settings.backup_folder.trim().to_string(),
        last_backup_at,
        meeting_language: settings.meeting_language.trim().to_string(),
        calendar_enabled: settings.calendar_enabled,
        notion_enabled: settings.notion_enabled,
        notion_parent_page: settings.notion_parent_page.trim().to_string(),
        notion_last_sync_at,
        raw_retention_days: settings.raw_retention_days.clamp(0, 3650),
        meeting_notifications: settings.meeting_notifications,
        mail_enabled: settings.mail_enabled,
        mail_memories: settings.mail_memories,
        contacts_enabled: settings.contacts_enabled,
        contacts_last_sync_at,
        clipboard_enabled: settings.clipboard_enabled,
        morning_brief_at: valid_time(&settings.morning_brief_at),
        evening_brief_at: valid_time(&settings.evening_brief_at),
        notch_enabled: settings.notch_enabled,
        thought_check_enabled: settings.thought_check_enabled,
        diarize_enabled: settings.diarize_enabled,
        screenshots_enabled: settings.screenshots_enabled,
        auto_listen_calls: settings.auto_listen_calls,
        notch_position: match settings.notch_position.as_str() { "top-left" | "top-right" | "bottom-center" | "left" | "right" => settings.notch_position.clone(), _ => "top-center".into() },
        notch_position_chosen: settings.notch_position_chosen,
        purpose_why: settings.purpose_why.trim().chars().take(1_000).collect(),
        purpose_how: tidy(settings.purpose_how).into_iter().take(7).map(|l| l.chars().take(160).collect()).collect(),
        signal_weights: settings.signal_weights,
    };
    let purpose_changed = {
        let cur = crate::lock(&state.settings);
        cur.purpose_why != clean.purpose_why || cur.purpose_how != clean.purpose_how
    };
    let old_position = crate::lock(&state.settings).notch_position.clone();
    let mail_changed = clean.mail_enabled != old_mail || clean.mail_memories != old_mail_memories;
    let contacts_on = clean.contacts_enabled && !old_contacts;
    if clean.index_folders != old_folders || clean.index_files != old_index_files {
        state.rescan_files.store(true, Ordering::Relaxed);
    }
    crate::lock(&state.store).save_settings(&clean).map_err(err)?;
    *crate::lock(&state.settings) = clean.clone();
    if mail_changed {
        crate::engine::apply_mail_setting(&state);
    }
    if contacts_on {
        crate::lock(&state.settings).contacts_last_sync_at = 0;
        state.engine_wake.store(true, Ordering::Relaxed);
    }
    if clean.notch_enabled != old_notch {
        crate::notch_visible(&app, clean.notch_enabled);
    }
    if clean.notch_position != old_position {
        let _ = app.emit("notch-position", clean.notch_position.clone());
        crate::place_notch(&app);
    }
    if purpose_changed {
        // A new why re-ranks today at once.
        let st = Arc::clone(&state);
        let app2 = app.clone();
        std::thread::spawn(move || {
            let now = crate::capture::now_ms();
            let _ = crate::signals::refresh(&st, now);
            let _ = crate::signals::alignment_day(&st, now);
            let _ = app2.emit("signals-changed", ());
        });
    }
    Ok(clean)
}

#[tauri::command(async)]
pub fn wipe_all(app: AppHandle, state: State<'_, Arc<AppState>>) -> Res<()> {
    crate::lock(&state.store).wipe().map_err(err)?;
    let _ = app.emit("memories-changed", ());
    let _ = app.emit("activity-changed", ());
    Ok(())
}

// ── Memories ─────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn list_memories(state: State<'_, Arc<AppState>>, query: Option<String>, kept_only: Option<bool>, limit: Option<u32>, dropped_only: Option<bool>) -> Res<Vec<MemoryCard>> {
    let mut cards = list_memories_inner(&state, query, kept_only, limit)?;
    if dropped_only.unwrap_or(false) {
        // The bin: what Rabbit judged not worth keeping and you have not rescued.
        cards.retain(|c| c.feedback.as_deref() == Some("ignore") || (!c.keep && c.feedback.is_none()));
    }
    crate::lock(&state.store).attach_facts(&mut cards).map_err(err)?;
    Ok(cards)
}

fn list_memories_inner(state: &State<'_, Arc<AppState>>, query: Option<String>, kept_only: Option<bool>, limit: Option<u32>) -> Res<Vec<MemoryCard>> {
    let query = query.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let kept_only = kept_only.unwrap_or(true);
    let limit = limit.unwrap_or(200).min(1000);
    match query {
        None => crate::lock(&state.store).list_memories(None, kept_only, limit).map_err(err),
        Some(q) => {
            // Semantic search when the index is up; keyword otherwise.
            let qvec = crate::engine::embed_query(state, q);
            crate::lock(&state.store).search_memories(q, qvec.as_deref(), kept_only, limit).map_err(err)
        }
    }
}

#[tauri::command(async)]
pub fn memory_feedback(state: State<'_, Arc<AppState>>, ids: Vec<i64>, feedback: Option<String>) -> Res<()> {
    let f = feedback.as_deref().filter(|f| *f == "keep" || *f == "ignore");
    crate::lock(&state.store).set_memory_feedback(&ids, f).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineReport {
    #[serde(flatten)]
    status: EngineStatus,
    counts: MemoryCounts,
}

#[tauri::command(async)]
pub fn engine_status(state: State<'_, Arc<AppState>>) -> Res<EngineReport> {
    let status = crate::lock(&state.engine).clone();
    let counts = crate::lock(&state.store).memory_counts().map_err(err)?;
    Ok(EngineReport { status, counts })
}

#[tauri::command(async)]
pub fn process_now(state: State<'_, Arc<AppState>>) {
    state.engine_wake.store(true, Ordering::Relaxed);
}

/// Writes every thumbed memory as JSON lines to the Desktop; returns the path.
#[tauri::command(async)]
pub fn export_labels(state: State<'_, Arc<AppState>>) -> Res<String> {
    let text = crate::lock(&state.store).export_labels().map_err(err)?;
    let home = std::env::var("HOME").unwrap_or_default();
    let path = std::path::PathBuf::from(home).join("Desktop").join("lane-labels.jsonl");
    std::fs::write(&path, text).map_err(err)?;
    Ok(path.display().to_string())
}

// ── Ask, recap, day ──────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskStart {
    id: u64,
    conversation_id: Option<i64>,
}

/// Start answering; tokens arrive as `ask-token` events, the result as `ask-done`.
#[tauri::command(async)]
pub fn ask(app: AppHandle, state: State<'_, Arc<AppState>>, question: String, history: Option<Vec<crate::engine::AskTurn>>, mode: Option<String>, conversation_id: Option<i64>, persist: Option<bool>) -> Res<AskStart> {
    let id = ASK_SEQ.fetch_add(1, Ordering::Relaxed);
    let draft = mode.as_deref() == Some("draft");
    let now = crate::capture::now_ms();
    // Every question asked in the Ask page is kept in a conversation, like a
    // chat history; the overlay and Today pass persist=false.
    let conversation = if persist.unwrap_or(true) {
        let store = crate::lock(&state.store);
        let c = match conversation_id {
            Some(c) => c,
            None => store.create_conversation(&question, now).map_err(err)?,
        };
        store.append_message(c, "user", &question, "[]", now).map_err(err)?;
        Some(c)
    } else {
        None
    };
    crate::engine::ask(app, Arc::clone(&state), id, question, history.unwrap_or_default(), draft, conversation);
    Ok(AskStart { id, conversation_id: conversation })
}

/// Streams the briefing for the day / week / month containing `ms` as
/// `ask-token` events with this id, and finishes with `ask-done`.
#[tauri::command(async)]
pub fn recap(app: AppHandle, state: State<'_, Arc<AppState>>, ms: i64, force: Option<bool>, week: Option<bool>, span: Option<String>) -> u64 {
    let id = ASK_SEQ.fetch_add(1, Ordering::Relaxed);
    let state = Arc::clone(&state);
    let span = span.unwrap_or_else(|| if week.unwrap_or(false) { "week".into() } else { "day".into() });
    std::thread::spawn(move || {
        let result = crate::engine::recap_span(&state, ms, &span, force.unwrap_or(false), |t| {
            let _ = app.emit("ask-token", serde_json::json!({"id": id, "token": t}));
        });
        let payload = match result {
            Ok((answer, sources)) => crate::engine::AskResult { id, answer, sources, error: None, followups: vec![], unverified: vec![], checked: false, scope_fixed: false },
            Err(e) => crate::engine::AskResult { id, answer: String::new(), sources: vec![], error: Some(e), followups: vec![], unverified: vec![], checked: false, scope_fixed: false },
        };
        let _ = app.emit("ask-done", payload);
    });
    id
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayStats {
    memories: usize,
    open_tasks: Vec<Task>,
    /// All open commitments from the week (the list above is capped).
    open_task_count: i64,
    time_by_app: Vec<(String, i64)>,
}

#[tauri::command(async)]
pub fn day_stats(state: State<'_, Arc<AppState>>, ms: i64) -> Res<DayStats> {
    let (start, end) = crate::engine::day_bounds(ms);
    let store = crate::lock(&state.store);
    Ok(DayStats {
        memories: store.memories_between(start, end, 200).map_err(err)?.len(),
        open_tasks: store.tasks_between(start - 7 * 86_400_000, end).map_err(err)?,
        open_task_count: store.open_task_count(start - 7 * 86_400_000, end).map_err(err)?,
        time_by_app: store.time_by_app(start, end).map_err(err)?,
    })
}

#[tauri::command(async)]
pub fn add_note(app: AppHandle, state: State<'_, Arc<AppState>>, text: String) -> Res<i64> {
    if text.trim().is_empty() {
        return Err("Write something first".into());
    }
    let id = crate::lock(&state.store).create_note(text.trim(), crate::capture::now_ms(), "Note").map_err(err)?;
    state.engine_wake.store(true, Ordering::Relaxed);
    let _ = app.emit("activity-changed", ());
    Ok(id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    text: String,
    title: String,
    activity_id: i64,
    at: i64,
}

#[tauri::command(async)]
pub fn list_decisions(state: State<'_, Arc<AppState>>, since: Option<i64>) -> Res<Vec<Decision>> {
    let now = crate::capture::now_ms();
    let rows = crate::lock(&state.store).decisions_between(since.unwrap_or(now - 30 * 86_400_000), now + 86_400_000).map_err(err)?;
    Ok(rows.into_iter().map(|(text, title, activity_id, at)| Decision { text, title, activity_id, at }).collect())
}

// ── Entities, graph, board, tasks ────────────────────────────────────────

#[tauri::command(async)]
pub fn list_entities(state: State<'_, Arc<AppState>>, kind: Option<String>, query: Option<String>, limit: Option<u32>) -> Res<Vec<Entity>> {
    crate::lock(&state.store)
        .list_entities(kind.as_deref().filter(|k| !k.is_empty()), query.as_deref().filter(|q| !q.trim().is_empty()), limit.unwrap_or(100).min(1000))
        .map_err(err)
}

#[tauri::command(async)]
pub fn entity_memories(state: State<'_, Arc<AppState>>, id: i64, limit: Option<u32>) -> Res<Vec<MemoryCard>> {
    crate::lock(&state.store).entity_memories(id, limit.unwrap_or(50)).map_err(err)
}

#[tauri::command(async)]
pub fn graph(state: State<'_, Arc<AppState>>, max_nodes: Option<u32>, min_mentions: Option<i64>) -> Res<Graph> {
    crate::lock(&state.store).graph(max_nodes.unwrap_or(150).min(1000), min_mentions.unwrap_or(1)).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardMemoryNode {
    card: MemoryCard,
    entity_ids: Vec<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardData {
    #[serde(flatten)]
    graph: Graph,
    memories: Vec<BoardMemoryNode>,
    positions: Vec<(String, f64, f64)>,
    links: Vec<crate::store::BoardLink>,
    notes: Vec<crate::store::BoardNote>,
    /// Memories placed on the board by hand (not in the time range).
    placed: Vec<MemoryCard>,
}

/// Everything the Board draws: entities, their co-occurrence edges, memory
/// groups in the time range with the entities they mention, pinned positions.
#[tauri::command(async)]
pub fn board(state: State<'_, Arc<AppState>>, since: Option<i64>, max_nodes: Option<u32>, min_mentions: Option<i64>, max_memories: Option<u32>) -> Res<BoardData> {
    let store = crate::lock(&state.store);
    let graph = store.graph(max_nodes.unwrap_or(200).min(1000), min_mentions.unwrap_or(1)).map_err(err)?;
    let memories = store
        .board_memories(since.unwrap_or(0), max_memories.unwrap_or(120).min(500))
        .map_err(err)?
        .into_iter()
        .map(|(card, entity_ids)| BoardMemoryNode { card, entity_ids })
        .collect();
    let positions = store.board_positions().map_err(err)?;
    let links = store.board_links().map_err(err)?;
    let notes = store.board_notes().map_err(err)?;
    let mut placed = Vec::new();
    for (key, _, _) in &positions {
        if let Some(id) = key.strip_prefix("m:").and_then(|x| x.parse::<i64>().ok()) {
            if let Ok(Some(card)) = store.memory_by_id(id) {
                placed.push(card);
            }
        }
    }
    Ok(BoardData { graph, memories, positions, links, notes, placed })
}

#[tauri::command(async)]
pub fn set_board_position(state: State<'_, Arc<AppState>>, key: String, x: Option<f64>, y: Option<f64>) -> Res<()> {
    let pos = match (x, y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    crate::lock(&state.store).set_board_position(&key, pos).map_err(err)
}

#[tauri::command(async)]
pub fn add_board_link(state: State<'_, Arc<AppState>>, from_key: String, to_key: String, label: Option<String>) -> Res<i64> {
    if from_key == to_key {
        return Err("A node cannot link to itself".into());
    }
    crate::lock(&state.store).add_board_link(&from_key, &to_key, label.as_deref().unwrap_or(""), crate::capture::now_ms()).map_err(err)
}

#[tauri::command(async)]
pub fn set_board_link_label(state: State<'_, Arc<AppState>>, id: i64, label: String) -> Res<()> {
    crate::lock(&state.store).set_board_link_label(id, &label).map_err(err)
}

#[tauri::command(async)]
pub fn remove_board_link(state: State<'_, Arc<AppState>>, id: i64) -> Res<()> {
    crate::lock(&state.store).remove_board_link(id).map_err(err)
}

#[tauri::command(async)]
pub fn set_board_note(state: State<'_, Arc<AppState>>, key: String, text: String, x: f64, y: f64) -> Res<()> {
    let store = crate::lock(&state.store);
    store.set_board_note(&key, &text, crate::capture::now_ms()).map_err(err)?;
    store.set_board_position(&key, Some((x, y))).map_err(err)
}

#[tauri::command(async)]
pub fn remove_board_node(state: State<'_, Arc<AppState>>, key: String) -> Res<()> {
    crate::lock(&state.store).remove_board_node(&key).map_err(err)
}

#[tauri::command(async)]
pub fn list_tasks(state: State<'_, Arc<AppState>>, status: Option<String>, limit: Option<u32>) -> Res<Vec<Task>> {
    crate::lock(&state.store).list_tasks(status.as_deref().unwrap_or("open"), limit.unwrap_or(200).min(1000)).map_err(err)
}

#[tauri::command(async)]
pub fn set_task_status(state: State<'_, Arc<AppState>>, id: i64, status: String) -> Res<()> {
    if !["open", "done", "dismissed"].contains(&status.as_str()) {
        return Err("unknown status".into());
    }
    crate::lock(&state.store).set_task_status(id, &status, crate::capture::now_ms()).map_err(err)
}

// ── Files ────────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn search_files(state: State<'_, Arc<AppState>>, query: String, limit: Option<u32>) -> Res<Vec<FileHit>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let qvec = crate::engine::embed_query(&state, q);
    crate::lock(&state.store).search_files(q, qvec.as_deref(), limit.unwrap_or(20).min(200)).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReport {
    #[serde(flatten)]
    stats: FileStats,
    folders: Vec<String>,
    pending: i64,
    detail: String,
}

#[tauri::command(async)]
pub fn file_stats(state: State<'_, Arc<AppState>>) -> Res<FileReport> {
    let stats = crate::lock(&state.store).file_stats().map_err(err)?;
    let folders = {
        let s = crate::lock(&state.settings);
        if s.index_folders.is_empty() { crate::files::default_folders() } else { s.index_folders.clone() }
    };
    let (pending, detail) = {
        let e = crate::lock(&state.engine);
        (e.files_pending, e.files_detail.clone())
    };
    Ok(FileReport { stats, folders, pending, detail })
}

/// "Speaker 2" → "Sarah" throughout a meeting's transcript.
#[tauri::command(async)]
pub fn rename_speaker(app: AppHandle, state: State<'_, Arc<AppState>>, id: i64, label: String, name: String) -> Res<usize> {
    let name = name.trim();
    if name.is_empty() || name.contains(':') {
        return Err("Enter a name".into());
    }
    let n = crate::lock(&state.store).rename_speaker(id, label.trim(), name).map_err(err)?;
    state.engine_wake.store(true, Ordering::Relaxed);
    let _ = app.emit("meetings-changed", ());
    let _ = app.emit("memories-changed", ());
    Ok(n)
}

/// The picture kept with a snapshot, as a data URL.
#[tauri::command(async)]
pub fn snapshot_image(state: State<'_, Arc<AppState>>, path: String) -> Res<String> {
    let data_dir = state.db_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    crate::shots::data_url(&data_dir, &path)
}

#[tauri::command(async)]
pub fn request_screen_recording() -> bool {
    crate::shots::request()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerToolkit {
    ready: bool,
    detail: String,
}

/// Where the speaker toolkit stands; `fetch` downloads it now (≈60 MB once).
#[tauri::command(async)]
pub fn speaker_toolkit(state: State<'_, Arc<AppState>>, fetch: Option<bool>) -> Res<SpeakerToolkit> {
    let data_dir = state.db_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    if fetch.unwrap_or(false) && !crate::diarize::ready(&data_dir) {
        crate::diarize::ensure(&data_dir, |m| log::info!("speakers: {m}"))?;
    }
    let ready = crate::diarize::ready(&data_dir);
    Ok(SpeakerToolkit { ready, detail: if ready { "Ready".into() } else { "Downloads about 60 MB the first time it is used".into() } })
}

#[tauri::command(async)]
pub fn list_files(state: State<'_, Arc<AppState>>, limit: Option<u32>) -> Res<Vec<FileHit>> {
    crate::lock(&state.store).list_files(limit.unwrap_or(40).min(500)).map_err(err)
}

#[tauri::command(async)]
pub fn reveal_file(path: String) -> Res<()> {
    crate::files::reveal(&path)
}

#[tauri::command(async)]
pub fn open_file(path: String) -> Res<()> {
    crate::files::open_file(&path)
}

#[tauri::command(async)]
pub fn reindex_files(state: State<'_, Arc<AppState>>) {
    state.rescan_files.store(true, Ordering::Relaxed);
    state.engine_wake.store(true, Ordering::Relaxed);
}

// ── Meetings ─────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn start_meeting(app: AppHandle, state: State<'_, Arc<AppState>>, title: Option<String>, voice_note: Option<bool>) -> Res<i64> {
    crate::engine::start_meeting(&app, &state, title.unwrap_or_default(), voice_note.unwrap_or(false))
}

#[tauri::command(async)]
pub fn stop_meeting(app: AppHandle, state: State<'_, Arc<AppState>>) -> Res<()> {
    crate::engine::stop_meeting(app, Arc::clone(&state));
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingReport {
    #[serde(flatten)]
    status: RecordingStatus,
    mic_seconds: i64,
    system_seconds: i64,
    ready: bool,
    detail_ready: String,
    /// True while ⌥⇧Space dictation is listening (the mic is in use, but it is not a meeting).
    dictating: bool,
}

#[tauri::command(async)]
pub fn recording_status(state: State<'_, Arc<AppState>>) -> Res<RecordingReport> {
    let status = crate::lock(&state.recording).clone();
    let (ready, detail_ready) = crate::engine::meeting_readiness(&state);
    let dictating = crate::dictation::active();
    Ok(RecordingReport {
        status: RecordingStatus { recording: status.recording && !dictating, ..status },
        mic_seconds: crate::meetings::MIC_FRAMES.load(Ordering::Relaxed) / 16_000,
        system_seconds: crate::meetings::SYSTEM_FRAMES.load(Ordering::Relaxed) / 16_000,
        ready,
        detail_ready,
        dictating,
    })
}

#[tauri::command(async)]
pub fn list_meetings(state: State<'_, Arc<AppState>>, limit: Option<u32>) -> Res<Vec<Meeting>> {
    crate::lock(&state.store).list_meetings(limit.unwrap_or(100).min(1000)).map_err(err)
}

#[tauri::command(async)]
pub fn rename_meeting(app: AppHandle, state: State<'_, Arc<AppState>>, id: i64, title: String) -> Res<()> {
    if title.trim().is_empty() {
        return Err("A meeting needs a name".into());
    }
    crate::lock(&state.store).rename_meeting(id, title.trim()).map_err(err)?;
    let _ = app.emit("meetings-changed", ());
    Ok(())
}

#[tauri::command(async)]
pub fn meeting_summary(state: State<'_, Arc<AppState>>, id: i64, transcript: Option<bool>) -> Res<String> {
    crate::engine::meeting_summary(&state, id, transcript.unwrap_or(false))
}

/// Write (or rewrite) the notes for a finished meeting; `meetings-changed` follows.
#[tauri::command(async)]
pub fn meeting_notes(app: AppHandle, state: State<'_, Arc<AppState>>, id: i64) -> Res<()> {
    let state = Arc::clone(&state);
    std::thread::spawn(move || {
        match crate::engine::meeting_notes(&state, id) {
            Ok(_) => {}
            Err(e) => log::warn!("meeting {id}: notes: {e}"),
        }
        let _ = app.emit("meetings-changed", ());
    });
    Ok(())
}

/// Play the kept recording (your side) in the default audio app.
#[tauri::command(async)]
pub fn open_meeting_audio(state: State<'_, Arc<AppState>>, id: i64, reveal: Option<bool>) -> Res<()> {
    let dir = crate::lock(&state.store).meeting_audio_dir(id).map_err(err)?.ok_or("no recording for this meeting")?;
    let mic = std::path::Path::new(&dir).join("mic.wav");
    if !mic.is_file() {
        return Err("The audio was not kept. Turn on Keep audio before the next meeting.".into());
    }
    if reveal.unwrap_or(false) {
        crate::files::reveal(&mic.display().to_string())
    } else {
        crate::files::open_file(&mic.display().to_string())
    }
}

#[tauri::command(async)]
pub fn delete_meeting(app: AppHandle, state: State<'_, Arc<AppState>>, id: i64) -> Res<()> {
    let (activity, dir) = {
        let store = crate::lock(&state.store);
        (store.meeting_activity(id).map_err(err)?, store.meeting_audio_dir(id).map_err(err)?)
    };
    crate::lock(&state.store).delete_activity(activity).map_err(err)?;
    if let Some(d) = dir {
        let _ = std::fs::remove_dir_all(d);
    }
    let _ = app.emit("meetings-changed", ());
    let _ = app.emit("memories-changed", ());
    Ok(())
}

#[tauri::command]
pub fn copy_text(text: String) -> Res<()> {
    let mut board = arboard::Clipboard::new().map_err(err)?;
    board.set_text(text).map_err(err)
}

/// Write a Markdown file: into the Markdown folder's Lane/Notes when one is
/// set, otherwise Downloads. Returns the path.
#[tauri::command(async)]
pub fn save_markdown(state: State<'_, Arc<AppState>>, name: String, text: String) -> Res<String> {
    let folder = crate::lock(&state.settings).markdown_folder.clone();
    let dir = if folder.trim().is_empty() {
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Downloads")
    } else {
        std::path::PathBuf::from(folder.trim()).join("Lane").join("Notes")
    };
    std::fs::create_dir_all(&dir).map_err(err)?;
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { ' ' }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let safe = if safe.is_empty() { "Lane".to_string() } else { safe.chars().take(80).collect() };
    let path = dir.join(format!("{safe}.md"));
    std::fs::write(&path, text).map_err(err)?;
    Ok(path.display().to_string())
}

// ── Labels ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelsStatus {
    enabled: bool,
    unsent: Vec<(i64, String, String)>,
    last_sent_at: i64,
    detail: String,
    endpoint: String,
}

#[tauri::command(async)]
pub fn labels_status(state: State<'_, Arc<AppState>>) -> Res<LabelsStatus> {
    let s = crate::lock(&state.settings).clone();
    Ok(LabelsStatus {
        enabled: s.contribute_labels,
        unsent: crate::lock(&state.store).unsent_label_ids().map_err(err)?,
        last_sent_at: s.labels_last_sent_at,
        detail: crate::lock(&state.engine).labels_detail.clone(),
        endpoint: crate::engine::LABELS_ENDPOINT.into(),
    })
}

/// "Never send this one": marked as sent without sending.
#[tauri::command(async)]
pub fn exclude_label(state: State<'_, Arc<AppState>>, id: i64) -> Res<()> {
    crate::lock(&state.store).mark_labels_sent(&[id], crate::capture::now_ms()).map_err(err)
}

#[tauri::command(async)]
pub fn send_labels_now(state: State<'_, Arc<AppState>>) -> Res<usize> {
    crate::engine::send_labels(&state)
}

// ── Backups ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    enabled: bool,
    folder: String,
    last_backup_at: i64,
    latest_file: Option<String>,
    has_passphrase: bool,
    detail: String,
}

fn backup_folder(state: &AppState) -> std::path::PathBuf {
    let f = crate::lock(&state.settings).backup_folder.clone();
    if f.trim().is_empty() { crate::backup::default_folder() } else { std::path::PathBuf::from(f.trim()) }
}

#[tauri::command(async)]
pub fn backup_status(state: State<'_, Arc<AppState>>) -> Res<BackupStatus> {
    let folder = backup_folder(&state);
    let s = crate::lock(&state.settings).clone();
    Ok(BackupStatus {
        enabled: s.backup_enabled,
        folder: folder.display().to_string(),
        last_backup_at: s.last_backup_at,
        latest_file: crate::backup::latest(&folder).map(|(p, _)| p.display().to_string()),
        has_passphrase: crate::vault::keychain_get(crate::vault::BACKUP_SERVICE).is_some(),
        detail: crate::lock(&state.engine).backup_detail.clone(),
    })
}

/// Set (or change) the passphrase; backups turn on. Returns the folder.
#[tauri::command(async)]
pub fn set_backup_passphrase(state: State<'_, Arc<AppState>>, passphrase: String) -> Res<String> {
    if passphrase.chars().count() < 8 {
        return Err("Use at least 8 characters".into());
    }
    crate::vault::keychain_set(crate::vault::BACKUP_SERVICE, &passphrase)?;
    {
        let mut s = crate::lock(&state.settings);
        s.backup_enabled = true;
        crate::lock(&state.store).save_settings(&s).map_err(err)?;
    }
    state.engine_wake.store(true, Ordering::Relaxed);
    Ok(backup_folder(&state).display().to_string())
}

#[tauri::command(async)]
pub fn backup_now(state: State<'_, Arc<AppState>>) -> Res<String> {
    crate::engine::run_backup(&state).map(|p| p.display().to_string())
}

#[tauri::command(async)]
pub fn disable_backups(state: State<'_, Arc<AppState>>) -> Res<()> {
    crate::vault::keychain_delete(crate::vault::BACKUP_SERVICE);
    let mut s = crate::lock(&state.settings);
    s.backup_enabled = false;
    crate::lock(&state.store).save_settings(&s).map_err(err)
}

/// A file picker for a `.rvault` backup (AppleScript, so no dialog plugin).
#[tauri::command]
pub fn choose_backup_file() -> Res<Option<String>> {
    let out = std::process::Command::new("osascript")
        .args(["-e", "POSIX path of (choose file with prompt \"Choose a Lane backup\" of type {\"rvault\"})"])
        .output()
        .map_err(err)?;
    if !out.status.success() {
        return Ok(None); // cancelled
    }
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if p.is_empty() { None } else { Some(p) })
}

/// Decrypt the backup next to the database, swap it in, and restart: the
/// new database is encrypted into the vault on the way up.
#[tauri::command(async)]
pub fn restore_backup(app: AppHandle, state: State<'_, Arc<AppState>>, path: String, passphrase: String) -> Res<()> {
    let restored = state.db_path.with_extension("db.restored");
    crate::backup::restore_to(std::path::Path::new(&path), &passphrase, &restored)?;
    let before = state.db_path.with_extension(format!("db.before-restore-{}", crate::capture::now_ms()));
    std::fs::rename(&state.db_path, &before).map_err(err)?;
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", state.db_path.display()));
    }
    std::fs::rename(&restored, &state.db_path).map_err(err)?;
    log::info!("restore: swapped in {} (previous kept as {})", path, before.display());
    app.restart();
}

// ── Windows ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn show_overlay(app: AppHandle) {
    crate::toggle_overlay(&app, Some(true));
}

#[tauri::command]
pub fn hide_overlay(app: AppHandle) {
    crate::toggle_overlay(&app, Some(false));
}

/// Opens the main window on a page (used by the overlay and the notch).
#[tauri::command]
pub fn open_main(app: AppHandle, page: Option<String>, activity: Option<i64>) {
    crate::toggle_overlay(&app, Some(false));
    crate::show_main(&app);
    let _ = app.emit("navigate", serde_json::json!({"page": page, "activity": activity}));
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    log::info!("quit from the profile menu");
    crate::runtime::shutdown();
    app.exit(0);
}

#[tauri::command]
pub fn notch_show(app: AppHandle) {
    crate::notch_visible(&app, true);
}

#[tauri::command]
pub fn notch_hide(app: AppHandle) {
    crate::notch_visible(&app, false);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastHere {
    title: String,
    summary: String,
    at: i64,
    activity_id: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonHere {
    id: i64,
    name: String,
    owed: Vec<String>,
    last_title: Option<String>,
    last_at: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotchContext {
    app: String,
    title: String,
    url: Option<String>,
    /// The last memory about this same page or document, from an earlier sitting.
    last: Option<LastHere>,
    /// A known person named in the window title: what you owe them, last contact.
    person: Option<PersonHere>,
    /// What this window connects to, from the co-occurrence graph: (name, memories together).
    connects: Vec<(String, i64)>,
    notch_apps: Vec<String>,
}

/// What the notch card says about the window in front: read when the card opens.
#[tauri::command(async)]
pub fn notch_context(state: State<'_, Arc<AppState>>) -> Res<NotchContext> {
    let read_web = crate::lock(&state.settings).read_browser_text;
    let settings = crate::lock(&state.settings).clone();
    let front = crate::capture::platform::observe(read_web, false).filter(|o| !privacy::is_excluded(&settings, &o.app_name, o.bundle_id.as_deref(), o.url.as_deref()));
    let notch_apps = crate::running_notch_apps();
    let Some(o) = front else { return Ok(NotchContext { app: String::new(), title: String::new(), url: None, last: None, person: None, connects: vec![], notch_apps }) };
    let now = crate::capture::now_ms();
    // The page's own title: the first segment before " - App" / " | Site".
    let head: String = o.window_title.split(|c| c == '|' || c == '–' || c == '—').next().unwrap_or("").trim().to_string();
    let head = head.rsplit_once(" - ").map(|(a, _)| a.trim().to_string()).unwrap_or(head);
    let store = crate::lock(&state.store);
    let mut last = None;
    if head.chars().count() >= 4 {
        if let Ok(cards) = store.search_memories(&head, None, true, 6) {
            last = cards.into_iter().find(|c| c.started_at < now - 3_600_000 && (c.app_name == o.app_name || o.url.as_deref().zip(c.url.as_deref()).map_or(false, |(a, b)| a == b))).map(|c| LastHere { title: c.title, summary: c.summary.chars().take(180).collect(), at: c.started_at, activity_id: c.activity_id });
        }
    }
    let mut person = None;
    for seg in o.window_title.split(|c| c == '-' || c == '|' || c == '–' || c == '—' || c == ',' || c == '(' || c == ')' || c == ':') {
        let seg = seg.trim().trim_end_matches(" (You)");
        if !crate::engine::looks_like_name(seg) {
            continue;
        }
        if let Ok(found) = store.list_entities(Some("person"), Some(seg), 3) {
            if let Some(e) = found.into_iter().find(|e| e.name.eq_ignore_ascii_case(seg)) {
                let owed = store.tasks_for_entity(e.id, 3).map(|ts| ts.into_iter().map(|t| t.text.chars().take(100).collect()).collect()).unwrap_or_default();
                let lastm = store.entity_memories(e.id, 1).ok().and_then(|v| v.into_iter().next());
                person = Some(PersonHere { id: e.id, name: e.name, owed, last_title: lastm.as_ref().map(|m| m.title.clone()), last_at: lastm.map(|m| m.started_at) });
                break;
            }
        }
    }
    drop(store);
    let connects = crate::signals::connects_to(&state, &o.window_title);
    Ok(NotchContext { app: o.app_name, title: o.window_title, url: o.url, last, person, connects, notch_apps })
}

// ── Signals: three things ────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalsReport {
    day: String,
    signals: Vec<crate::store::Signal>,
    /// (score 0..1, memories, near) for today, when a why is set.
    alignment: Option<(f64, i64, i64)>,
    why_set: bool,
}

#[tauri::command(async)]
pub fn signals(state: State<'_, Arc<AppState>>, ms: Option<i64>) -> Res<SignalsReport> {
    let now = ms.unwrap_or_else(crate::capture::now_ms);
    let day = crate::engine::day_of(now);
    let mut list = crate::lock(&state.store).signals_for(&day).map_err(err)?;
    if list.is_empty() && ms.is_none() {
        crate::signals::refresh(&state, now)?;
        list = crate::lock(&state.store).signals_for(&day).map_err(err)?;
    }
    let alignment = crate::lock(&state.store).alignment_days(1).ok().and_then(|v| v.into_iter().find(|(d, _, _, _)| *d == day)).map(|(_, s, m, n)| (s, m, n));
    let why_set = !crate::lock(&state.settings).purpose_why.trim().is_empty();
    Ok(SignalsReport { day, signals: list, alignment, why_set })
}

/// open | pinned | done | noise. Pinned and noise teach the scorer.
#[tauri::command(async)]
pub fn set_signal_state(app: AppHandle, state: State<'_, Arc<AppState>>, id: i64, new_state: String) -> Res<()> {
    if !["open", "pinned", "done", "noise"].contains(&new_state.as_str()) {
        return Err("unknown state".into());
    }
    let sig = crate::lock(&state.store).signal_by_id(id).map_err(err)?.ok_or("no such signal")?;
    crate::lock(&state.store).set_signal_state(id, &new_state).map_err(err)?;
    if sig.kind == "task" && new_state == "done" {
        let _ = crate::lock(&state.store).set_task_status(sig.ref_id, "done", crate::capture::now_ms());
    }
    crate::signals::learn(&state, &sig.features, &new_state);
    let _ = app.emit("signals-changed", ());
    Ok(())
}

/// The person's own order for today, ids first to last.
#[tauri::command(async)]
pub fn rank_signals(app: AppHandle, state: State<'_, Arc<AppState>>, ids: Vec<i64>) -> Res<()> {
    let day = crate::engine::day_of(crate::capture::now_ms());
    let before = crate::lock(&state.store).signals_for(&day).map_err(err)?;
    crate::lock(&state.store).rank_signals(&day, &ids).map_err(err)?;
    // Whatever moved into the top three was lifted by hand: learn from it.
    for (i, id) in ids.iter().take(3).enumerate() {
        if let Some(s) = before.iter().find(|s| s.id == *id) {
            if s.rank as usize > i + 1 && s.rank > 3 {
                crate::signals::learn(&state, &s.features, "up");
            }
        }
    }
    let _ = app.emit("signals-changed", ());
    Ok(())
}

#[tauri::command(async)]
pub fn refresh_signals(app: AppHandle, state: State<'_, Arc<AppState>>) -> Res<usize> {
    let now = crate::capture::now_ms();
    let n = crate::signals::refresh(&state, now)?;
    let _ = crate::signals::alignment_day(&state, now);
    let _ = app.emit("signals-changed", ());
    Ok(n)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CircleDot {
    id: i64,
    activity_id: i64,
    title: String,
    kind: String,
    at: i64,
    /// 0..1, closeness to the why.
    alignment: f64,
    project: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Circle {
    why: String,
    how: Vec<String>,
    projects: Vec<(String, i64)>,
    dots: Vec<CircleDot>,
    days: Vec<(String, f64, i64, i64)>,
}

/// The golden circle: why at the centre, how, what (projects), and the
/// month's memories placed by how close they sit to the why.
#[tauri::command(async)]
pub fn circle(state: State<'_, Arc<AppState>>) -> Res<Circle> {
    let (why, how) = {
        let s = crate::lock(&state.settings);
        (s.purpose_why.clone(), s.purpose_how.clone())
    };
    let now = crate::capture::now_ms();
    let text = format!("{}\n{}", why.trim(), how.join("\n"));
    let align: std::collections::HashMap<i64, f64> = if text.trim().chars().count() >= 12 {
        crate::engine::embed_query(&state, &text).map(|v| crate::lock(&state.store).vector_search(&v, 600).unwrap_or_default().into_iter().map(|(id, s)| (id, s.clamp(0.0, 1.0) as f64)).collect()).unwrap_or_default()
    } else {
        Default::default()
    };
    let store = crate::lock(&state.store);
    let cards = store.memories_between(now - 30 * 86_400_000, now + 86_400_000, 400).map_err(err)?;
    let projects: Vec<(String, i64)> = store.list_entities(Some("project"), None, 12).map_err(err)?.into_iter().map(|e| (e.name, e.mentions)).collect();
    let dots = cards
        .into_iter()
        .map(|c| {
            let project = c.projects.iter().find(|p| projects.iter().any(|(q, _)| q.eq_ignore_ascii_case(p))).cloned();
            CircleDot { id: c.id, activity_id: c.activity_id, title: c.title, kind: c.kind, at: c.started_at, alignment: align.get(&c.id).copied().unwrap_or(0.0), project }
        })
        .collect();
    let days = store.alignment_days(30).map_err(err)?;
    Ok(Circle { why, how, projects, dots, days })
}

/// The notch tab grows into a card and back; keep it centred either way.
#[tauri::command]
pub fn notch_resize(app: AppHandle, width: f64, height: f64) {
    if app.get_webview_window("notch").is_some() {
        crate::place_notch_size(&app, Some((width, height)));
    }
}

// ── Facts, pins, edits ───────────────────────────────────────────────────

#[tauri::command(async)]
pub fn set_pinned(state: State<'_, Arc<AppState>>, ids: Vec<i64>, pinned: bool) -> Res<()> {
    crate::lock(&state.store).set_pinned(&ids, pinned).map_err(err)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEdit {
    pub title: String,
    pub summary: String,
    pub people: Vec<String>,
    pub organizations: Vec<String>,
    pub projects: Vec<String>,
    pub decisions: Vec<String>,
}

#[tauri::command(async)]
pub fn update_memory(app: AppHandle, state: State<'_, Arc<AppState>>, id: i64, edit: MemoryEdit) -> Res<MemoryCard> {
    if edit.title.trim().is_empty() {
        return Err("A memory needs a title".into());
    }
    let now = crate::capture::now_ms();
    let store = crate::lock(&state.store);
    store
        .update_memory(id, &edit.title, &edit.summary, &tidy(edit.people), &tidy(edit.organizations), &tidy(edit.projects), &tidy(edit.decisions), now)
        .map_err(err)?;
    let mut card = store.memory_by_id(id).map_err(err)?.ok_or("memory not found")?;
    card.facts = store.facts_for(&[id]).map_err(err)?;
    drop(store);
    state.engine_wake.store(true, Ordering::Relaxed); // re-embed
    let _ = app.emit("memories-changed", ());
    Ok(card)
}

#[tauri::command(async)]
pub fn memory_facts(state: State<'_, Arc<AppState>>, ids: Vec<i64>) -> Res<Vec<crate::store::Fact>> {
    crate::lock(&state.store).facts_for(&ids).map_err(err)
}

#[tauri::command(async)]
pub fn add_fact(state: State<'_, Arc<AppState>>, memory_id: i64, subject: String, attribute: String, value: String) -> Res<i64> {
    if subject.trim().is_empty() || attribute.trim().is_empty() || value.trim().is_empty() {
        return Err("Subject, attribute and value are all needed".into());
    }
    crate::lock(&state.store).add_fact(memory_id, &subject, &attribute, &value, crate::capture::now_ms()).map_err(err)
}

#[tauri::command(async)]
pub fn retract_fact(state: State<'_, Arc<AppState>>, id: i64) -> Res<()> {
    crate::lock(&state.store).retract_fact(id).map_err(err)
}

#[tauri::command(async)]
pub fn correct_fact(state: State<'_, Arc<AppState>>, id: i64, value: String) -> Res<i64> {
    if value.trim().is_empty() {
        return Err("Enter the correct value".into());
    }
    crate::lock(&state.store).correct_fact(id, &value, crate::capture::now_ms()).map_err(err)
}

#[tauri::command(async)]
pub fn conflicting_facts(state: State<'_, Arc<AppState>>, limit: Option<u32>) -> Res<Vec<crate::store::Fact>> {
    crate::lock(&state.store).conflicting_facts(limit.unwrap_or(50)).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDetail {
    card: MemoryCard,
    text: String,
}

/// One memory with its captured text, for the board drawer.
#[tauri::command(async)]
pub fn memory_detail(state: State<'_, Arc<AppState>>, id: i64) -> Res<MemoryDetail> {
    let store = crate::lock(&state.store);
    let mut card = store.memory_by_id(id).map_err(err)?.ok_or("memory not found")?;
    card.facts = store.facts_for(&[id]).map_err(err)?;
    let text = store.activity_text(card.activity_id, 6_000).map_err(err)?;
    Ok(MemoryDetail { card, text })
}

// ── Integrations ─────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn upcoming_events(state: State<'_, Arc<AppState>>, hours: Option<u32>) -> Res<Vec<crate::integrations::Event>> {
    crate::engine::upcoming_events(&state, hours.unwrap_or(24).min(24 * 14))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotionStatus {
    enabled: bool,
    connected: bool,
    workspace: String,
    parent_page: String,
    pages: i64,
    last_sync_at: i64,
    detail: String,
}

#[tauri::command(async)]
pub fn notion_status(state: State<'_, Arc<AppState>>) -> Res<NotionStatus> {
    let s = crate::lock(&state.settings).clone();
    let token = crate::vault::keychain_get(crate::integrations::NOTION_SERVICE).filter(|t| !t.is_empty());
    Ok(NotionStatus {
        enabled: s.notion_enabled,
        connected: token.is_some(),
        workspace: crate::vault::keychain_get("so.lane.app.notion.workspace").unwrap_or_default(),
        parent_page: s.notion_parent_page,
        pages: crate::lock(&state.store).count_files_with_prefix("notion://").map_err(err)?,
        last_sync_at: s.notion_last_sync_at,
        detail: crate::lock(&state.engine).notion_detail.clone(),
    })
}

/// Store a personal integration token after checking it works; returns the
/// workspace name (or the bot's name) Notion reports.
#[tauri::command(async)]
pub fn set_notion_token(state: State<'_, Arc<AppState>>, token: String) -> Res<String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("Paste the token first".into());
    }
    let who = crate::integrations::notion_whoami(&token)?;
    crate::vault::keychain_set(crate::integrations::NOTION_SERVICE, &token)?;
    let _ = crate::vault::keychain_set("so.lane.app.notion.workspace", &who);
    {
        let mut s = crate::lock(&state.settings);
        s.notion_enabled = true;
        s.notion_last_sync_at = 0;
        crate::lock(&state.store).save_settings(&s).map_err(err)?;
    }
    state.engine_wake.store(true, Ordering::Relaxed);
    Ok(who)
}

#[tauri::command(async)]
pub fn disconnect_notion(state: State<'_, Arc<AppState>>) -> Res<()> {
    crate::vault::keychain_delete("so.lane.app.notion.workspace");
    crate::engine::disconnect_notion(&state)
}

#[tauri::command(async)]
pub fn sync_notion_now(state: State<'_, Arc<AppState>>) -> Res<()> {
    let state = Arc::clone(&state);
    std::thread::spawn(move || {
        if let Err(e) = crate::engine::sync_notion_now(&state) {
            log::warn!("notion: {e}");
        }
    });
    Ok(())
}

#[tauri::command(async)]
pub fn export_to_notion(state: State<'_, Arc<AppState>>, title: String, text: String) -> Res<String> {
    crate::engine::export_to_notion(&state, &title, &text)
}

/// Streams like `ask`: tokens as `ask-token`, the result as `ask-done`.
#[tauri::command(async)]
pub fn entity_profile(app: AppHandle, state: State<'_, Arc<AppState>>, id: i64, force: Option<bool>) -> u64 {
    let req = ASK_SEQ.fetch_add(1, Ordering::Relaxed);
    let state = Arc::clone(&state);
    std::thread::spawn(move || {
        let result = crate::engine::entity_profile(&state, id, force.unwrap_or(false), |t| {
            let _ = app.emit("ask-token", serde_json::json!({"id": req, "token": t}));
        });
        let payload = match result {
            Ok((answer, sources)) => crate::engine::AskResult { id: req, answer, sources, error: None, followups: vec![], unverified: vec![], checked: false, scope_fixed: false },
            Err(e) => crate::engine::AskResult { id: req, answer: String::new(), sources: vec![], error: Some(e), followups: vec![], unverified: vec![], checked: false, scope_fixed: false },
        };
        let _ = app.emit("ask-done", payload);
    });
    req
}

// ── Connectors + MCP ─────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn list_connectors(state: State<'_, Arc<AppState>>) -> Res<Vec<crate::connectors::RunReport>> {
    Ok(crate::engine::connector_reports(&state))
}

#[tauri::command(async)]
pub fn run_connector(state: State<'_, Arc<AppState>>, id: String) {
    crate::engine::run_connector_now(&state, &id);
}

#[tauri::command(async)]
pub fn open_connectors_folder(state: State<'_, Arc<AppState>>) -> Res<String> {
    let dir = crate::connectors::dir(state.db_path.parent().unwrap_or(std::path::Path::new(".")));
    crate::connectors::ensure_examples(&dir);
    crate::files::open_file(&dir.display().to_string())?;
    Ok(dir.display().to_string())
}

/// The key a connector's `{{secret}}` expands to; kept in the Keychain.
#[tauri::command(async)]
pub fn set_connector_secret(id: String, secret: String) -> Res<()> {
    let service = crate::connectors::secret_service(&id);
    if secret.trim().is_empty() {
        crate::vault::keychain_delete(&service);
        return Ok(());
    }
    crate::vault::keychain_set(&service, secret.trim())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    binary: String,
    present: bool,
    config: String,
    claude_desktop_file: String,
}

#[tauri::command(async)]
pub fn mcp_config(state: State<'_, Arc<AppState>>) -> Res<McpConfig> {
    let mut candidates = Vec::new();
    if let Some(r) = &state.resource_dir {
        candidates.push(r.join("mcp").join("lane-mcp"));
    }
    candidates.push(std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/target/release/lane-mcp")));
    let binary = candidates.iter().find(|p| p.is_file()).cloned().unwrap_or_else(|| candidates[0].clone());
    Ok(McpConfig {
        present: binary.is_file(),
        config: crate::mcp::client_config(&binary),
        binary: binary.display().to_string(),
        claude_desktop_file: format!("{}/Library/Application Support/Claude/claude_desktop_config.json", std::env::var("HOME").unwrap_or_default()),
    })
}

// ── Upcoming, gaps, explore, aliases, insights ───────────────────────────

#[tauri::command(async)]
pub fn upcoming_dates(state: State<'_, Arc<AppState>>, days: Option<i64>) -> Res<Vec<crate::store::Upcoming>> {
    crate::lock(&state.store).upcoming_dates(crate::capture::now_ms(), days.unwrap_or(30).clamp(1, 365)).map_err(err)
}

#[tauri::command(async)]
pub fn memory_gaps(state: State<'_, Arc<AppState>>) -> Res<Vec<crate::store::Gap>> {
    Ok(crate::engine::memory_gaps(&state))
}

#[tauri::command(async)]
pub fn explore(state: State<'_, Arc<AppState>>) -> Res<crate::store::Explore> {
    crate::lock(&state.store).explore(crate::capture::now_ms()).map_err(err)
}

#[tauri::command(async)]
pub fn insights(state: State<'_, Arc<AppState>>) -> Res<Vec<String>> {
    crate::lock(&state.store).insights(crate::capture::now_ms()).map_err(err)
}

#[tauri::command(async)]
pub fn set_alias(state: State<'_, Arc<AppState>>, entity_id: i64, alias: String) -> Res<Vec<String>> {
    let store = crate::lock(&state.store);
    store.set_alias(entity_id, &alias).map_err(err)?;
    store.aliases_of(entity_id).map_err(err)
}

#[tauri::command(async)]
pub fn remove_alias(state: State<'_, Arc<AppState>>, entity_id: i64, alias: String) -> Res<Vec<String>> {
    let store = crate::lock(&state.store);
    store.remove_alias(&alias).map_err(err)?;
    store.aliases_of(entity_id).map_err(err)
}

#[tauri::command(async)]
pub fn aliases_of(state: State<'_, Arc<AppState>>, entity_id: i64) -> Res<Vec<String>> {
    crate::lock(&state.store).aliases_of(entity_id).map_err(err)
}

// ── Forgetting ───────────────────────────────────────────────────────────

/// Remove a name or word from everything Lane holds. Irreversible.
#[tauri::command(async)]
pub fn forget_term(app: AppHandle, state: State<'_, Arc<AppState>>, term: String) -> Res<crate::store::ForgetReport> {
    if term.trim().chars().count() < 2 {
        return Err("Enter at least two characters".into());
    }
    let r = crate::lock(&state.store).forget_term(&term).map_err(err)?;
    log::info!("forget: removed a term from {} memories, {} snapshots, {} facts", r.memories, r.snapshots, r.facts);
    state.engine_wake.store(true, Ordering::Relaxed); // re-embed touched memories
    let _ = app.emit("memories-changed", ());
    Ok(r)
}

// ── Ask about what is on screen ──────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSummary {
    app_name: String,
    window_title: String,
    url: Option<String>,
    chars: usize,
}

#[tauri::command(async)]
pub fn screen_context(state: State<'_, Arc<AppState>>) -> Option<ScreenSummary> {
    crate::lock(&state.screen).as_ref().map(|c| ScreenSummary { app_name: c.app_name.clone(), window_title: c.window_title.clone(), url: c.url.clone(), chars: c.text.chars().count() })
}

/// Like `ask`, with the window that was in front when the overlay opened
/// as source [S]. Streams `ask-token` / `ask-done`.
#[tauri::command(async)]
pub fn ask_screen(app: AppHandle, state: State<'_, Arc<AppState>>, question: String, history: Option<Vec<crate::engine::AskTurn>>) -> u64 {
    let id = ASK_SEQ.fetch_add(1, Ordering::Relaxed);
    let state = Arc::clone(&state);
    std::thread::Builder::new()
        .name("ask-screen".into())
        .spawn(move || {
            state.ask_active.store(true, Ordering::Relaxed);
            let result = crate::engine::answer_about_screen(&state, &question, &history.unwrap_or_default(), |t| {
                let _ = app.emit("ask-token", serde_json::json!({"id": id, "token": t}));
            });
            state.ask_active.store(false, Ordering::Relaxed);
            let payload = match result {
                Ok((answer, sources)) => crate::engine::AskResult { id, answer, sources, error: None, followups: vec![], unverified: vec![], checked: false, scope_fixed: false },
                Err(e) => crate::engine::AskResult { id, answer: String::new(), sources: vec![], error: Some(e), followups: vec![], unverified: vec![], checked: false, scope_fixed: false },
            };
            let _ = app.emit("ask-done", payload);
        })
        .expect("spawn ask-screen");
    id
}

// ── Conversations ────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn list_conversations(state: State<'_, Arc<AppState>>, limit: Option<u32>) -> Res<Vec<crate::store::Conversation>> {
    crate::lock(&state.store).list_conversations(limit.unwrap_or(50)).map_err(err)
}

#[tauri::command(async)]
pub fn conversation_messages(state: State<'_, Arc<AppState>>, id: i64) -> Res<Vec<crate::store::Message>> {
    crate::lock(&state.store).conversation_messages(id).map_err(err)
}

#[tauri::command(async)]
pub fn rename_conversation(state: State<'_, Arc<AppState>>, id: i64, title: String) -> Res<()> {
    crate::lock(&state.store).rename_conversation(id, &title).map_err(err)
}

#[tauri::command(async)]
pub fn delete_conversation(state: State<'_, Arc<AppState>>, id: i64) -> Res<()> {
    crate::lock(&state.store).delete_conversation(id).map_err(err)
}

// ── Dictation ────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn dictation_toggle(app: AppHandle, state: State<'_, Arc<AppState>>) -> Res<String> {
    crate::dictation::toggle(&app, &state)
}

// ── Report a problem ─────────────────────────────────────────────────────

/// Writes a diagnostics file to the Desktop and shows it in Finder. Nothing
/// is sent: the user attaches it to their message if they want to. Screen
/// text and memories are not included; settings are, minus any tokens.
#[tauri::command(async)]
pub fn diagnostics_bundle(app: AppHandle, state: State<'_, Arc<AppState>>) -> Res<String> {
    let now = crate::capture::now_ms();
    let stamp = crate::engine::format_time(now).replace([':', ' '], "-");
    let home = std::env::var("HOME").unwrap_or_default();
    let path = std::path::PathBuf::from(&home).join("Desktop").join(format!("Lane-diagnostics-{stamp}.txt"));
    let mut out = String::new();
    out.push_str(&format!("Lane {} · {}\n", env!("CARGO_PKG_VERSION"), crate::engine::format_time(now)));
    let sw = std::process::Command::new("sw_vers").output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    out.push_str(&format!("{}RAM: {} GB · chip: {}\n\n", sw, crate::runtime::physical_ram_gb(), std::env::consts::ARCH));
    {
        let mut s = crate::lock(&state.settings).clone();
        s.tester_token = if s.tester_token.is_empty() { String::new() } else { "(set)".into() };
        s.notion_parent_page = if s.notion_parent_page.is_empty() { String::new() } else { "(set)".into() };
        out.push_str("SETTINGS\n");
        out.push_str(&serde_json::to_string_pretty(&s).unwrap_or_default());
        out.push_str("\n\n");
    }
    {
        let e = crate::lock(&state.engine);
        out.push_str(&format!("ENGINE\navailable={} busy={} model={} backend={} detail={:?} last_error={:?} processed={} tok/s={:.1}\nfiles indexed={} pending={} {:?}\n\n", e.available, e.busy, e.model, e.backend, e.detail, e.last_error, e.processed, e.tokens_per_second, e.files_indexed, e.files_pending, e.files_detail));
    }
    if let Ok(c) = crate::lock(&state.store).memory_counts() {
        out.push_str(&format!("COUNTS\npending={} memories={} kept={}\n\n", c.pending, c.memories, c.kept));
    }
    out.push_str(&format!("PERMISSIONS\n{}\n\n", serde_json::to_string_pretty(&permissions::report(state.stale_grant.load(Ordering::Relaxed))).unwrap_or_default()));
    out.push_str("WINDOWS\n");
    for label in ["main", "overlay", "notch"] {
        if let Some(w) = app.get_webview_window(label) {
            out.push_str(&crate::window_report(&w));
            out.push('\n');
        }
    }
    out.push('\n');
    for (name, lines) in [("lane.log", 400), ("llama.log", 60), ("llama-embed.log", 30)] {
        let p = state.db_path.with_file_name(name);
        if let Ok(text) = std::fs::read_to_string(&p) {
            let tail: Vec<&str> = text.lines().rev().take(lines).collect::<Vec<_>>().into_iter().rev().collect();
            out.push_str(&format!("{} (last {} lines)\n{}\n\n", name.to_uppercase(), tail.len(), tail.join("\n")));
        }
    }
    std::fs::write(&path, out).map_err(err)?;
    let _ = crate::files::reveal(&path.display().to_string());
    Ok(path.display().to_string())
}

// Keep the platform module referenced for builds without capture features.
#[allow(dead_code)]
fn _platform_link() -> bool {
    platform::is_trusted(false)
}

/// The icon for the source of a memory: the site's own, else the app's.
#[tauri::command]
pub fn source_icon(state: State<'_, Arc<AppState>>, app: String, url: Option<String>) -> Option<String> {
    // host of the page, without the scheme, port or www.
    let domain = url.as_deref().and_then(|u| {
        let rest = u.split("://").nth(1).unwrap_or(u);
        let host = rest.split('/').next().unwrap_or("").split('@').last().unwrap_or("");
        let host = host.split(':').next().unwrap_or("");
        let host = host.trim_start_matches("www.");
        if host.contains('.') { Some(host.to_string()) } else { None }
    });
    let dir = state.db_path.parent()?.to_path_buf();
    crate::icons::source_icon(&app, domain.as_deref(), &dir)
}

/// True macOS full screen for the main window, used by the board's walkthrough.
#[tauri::command]
pub fn set_fullscreen(app: AppHandle, on: bool) -> Res<()> {
    if let Some(w) = app.get_webview_window("main") {
        w.set_fullscreen(on).map_err(err)?;
    }
    Ok(())
}

/// Record the screen for a fixed number of seconds with the system recorder,
/// used by the board to save a walkthrough. macOS asks for Screen Recording
/// the first time; the file is written where the person chose and nothing
/// leaves the Mac.
#[tauri::command(async)]
pub fn record_screen(state: State<'_, Arc<AppState>>, seconds: u32, name: String) -> Res<String> {
    let seconds = seconds.clamp(3, 15 * 60);
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '-' }).collect();
    let dir = state.db_path.with_file_name("walks");
    std::fs::create_dir_all(&dir).map_err(err)?;
    let out = dir.join(format!("{}.mov", safe.trim()));
    let status = std::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", "-v", &format!("-V{seconds}")])
        .arg(&out)
        .status()
        .map_err(|e| format!("could not start the recorder: {e}"))?;
    if !status.success() {
        return Err("macOS would not record the screen. Give Lane Screen Recording in System Settings → Privacy & Security.".into());
    }
    if !out.is_file() {
        return Err("The recording did not produce a file.".into());
    }
    Ok(out.display().to_string())
}

/// Capture what is on screen right now, because the person asked for it.
///
/// The background pass is careful: it skips excluded apps, waits for an
/// activity to settle and drops what looks like noise. This does none of
/// that. Whatever is in front is written down at once, marked so the memory
/// is kept, and Rabbit is woken to turn it into a memory immediately.
#[tauri::command(async)]
pub fn capture_now(app: AppHandle, state: State<'_, Arc<AppState>>) -> Res<String> {
    capture_now_inner(&app, &state)
}

pub fn capture_now_inner(app: &AppHandle, state: &Arc<AppState>) -> Res<String> {
    let read_web = crate::lock(&state.settings).read_browser_text;
    // Clicking the menu bar makes Lane itself the front app, and Lane never
    // captures Lane. Give macOS a moment to hand focus back to whatever was
    // in front, then look.
    // Pressed from Lane's own menu bar, so look past Lane to whatever is
    // behind it; failing that, the last thing the capture loop saw.
    let obs = crate::capture::platform::observe_front_other(read_web, true)
        .or_else(|| crate::lock(&state.last_seen).clone())
        .ok_or("Nothing in front to capture.")?;
    let now = crate::capture::now_ms();
    let title = if obs.window_title.trim().is_empty() { obs.app_name.clone() } else { obs.window_title.clone() };
    let text = obs.text.unwrap_or_default();
    let activity = {
        let store = crate::lock(&state.store);
        let id = store
            .start_activity(&obs.app_name, obs.app_path.as_deref(), &obs.window_title, obs.url.as_deref(), now)
            .map_err(err)?;
        // It ends now — the times stay true — and it is marked urgent so the
        // engine does not make it wait out the two-minute settling window.
        let _ = store.extend_activity(id, now);
        let _ = store.mark_activity_urgent(id);
        // Always store something content-bearing. The cleaner drops captures
        // that come out empty, and an activity with no snapshot is never made
        // into a memory — which is exactly how an asked-for capture went
        // missing before.
        let header = match obs.url.as_deref() {
            Some(u) => format!("Captured on request from {} · {title}\n{u}", obs.app_name),
            None => format!("Captured on request from {} · {title}", obs.app_name),
        };
        let body = if text.trim().is_empty() { header.clone() } else { format!("{header}\n\n{text}") };
        if store.add_snapshot(id, &body, now).ok().flatten().is_none() {
            log::warn!("capture: nothing worth keeping in {title}");
        }
        id
    };
    state.engine_wake.store(true, Ordering::Relaxed);
    let _ = app.emit("memories-changed", ());
    log::info!("capture: asked for {activity} · {title}");
    Ok(title)
}

/// Save a picture the app drew (the shareable card) to the Desktop.
#[tauri::command(async)]
pub fn save_png(name: String, data_url: String) -> Res<String> {
    let b64 = data_url.split(",").nth(1).ok_or("not an image")?;
    let bytes = b64_decode(b64).ok_or("could not read the image")?;
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '-' }).collect();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).ok_or("no home folder")?;
    let out = home.join("Desktop").join(format!("{}.png", safe.trim()));
    std::fs::write(&out, bytes).map_err(err)?;
    Ok(out.display().to_string())
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let clean: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut acc = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            acc |= val(*c)? << (18 - 6 * i);
        }
        let n = chunk.len();
        out.push((acc >> 16) as u8);
        if n > 2 { out.push((acc >> 8) as u8); }
        if n > 3 { out.push(acc as u8); }
    }
    Some(out)
}

/// Where this Mac stands: trial, licensed, or out of time.
#[tauri::command]
pub fn licence_status() -> crate::licence::Licence {
    crate::licence::status()
}

/// Paste the key from a receipt.
#[tauri::command]
pub fn apply_licence(app: AppHandle, key: String) -> Res<crate::licence::Licence> {
    let out = crate::licence::apply(&key)?;
    let _ = app.emit("licence-changed", ());
    Ok(out)
}

/// Take the key off this Mac, for moving to another one.
#[tauri::command]
pub fn clear_licence(app: AppHandle) -> crate::licence::Licence {
    let out = crate::licence::clear();
    let _ = app.emit("licence-changed", ());
    out
}
