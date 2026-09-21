//! Local memory store: one SQLite file on the user's Mac.
//!
//! Activities are sessions of attention (one app + window + URL while it
//! stays in front). Snapshots are distinct text states captured inside an
//! activity. Both are indexed with FTS5 external-content tables.
//!
//! FTS triggers must pass `rowid` explicitly. Without it the index desyncs
//! on the first delete and SQLite reports "database disk image is
//! malformed" (the bug found in the old Rabbit store). Tests below cover it.

use crate::clean;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS activities (
    id             INTEGER PRIMARY KEY,
    app_name       TEXT NOT NULL,
    app_path       TEXT,
    window_title   TEXT NOT NULL DEFAULT '',
    url            TEXT,
    started_at     INTEGER NOT NULL,
    ended_at       INTEGER NOT NULL,
    snapshot_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_activities_started ON activities(started_at);

CREATE TABLE IF NOT EXISTS snapshots (
    id          INTEGER PRIMARY KEY,
    activity_id INTEGER NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    captured_at INTEGER NOT NULL,
    text        TEXT NOT NULL,
    text_hash   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_snapshots_activity ON snapshots(activity_id);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- One memory per processed activity: what the on-device model made of it.
CREATE TABLE IF NOT EXISTS memories (
    id            INTEGER PRIMARY KEY,
    activity_id   INTEGER NOT NULL UNIQUE REFERENCES activities(id) ON DELETE CASCADE,
    created_at    INTEGER NOT NULL,
    kind          TEXT NOT NULL,
    title         TEXT NOT NULL,
    summary       TEXT NOT NULL,
    people        TEXT NOT NULL DEFAULT '[]',
    organizations TEXT NOT NULL DEFAULT '[]',
    dates         TEXT NOT NULL DEFAULT '[]',
    numbers       TEXT NOT NULL DEFAULT '[]',
    keep          INTEGER NOT NULL,
    confidence    REAL NOT NULL,
    dropped       INTEGER NOT NULL DEFAULT 0,
    model         TEXT NOT NULL,
    feedback      TEXT,
    group_key     TEXT NOT NULL DEFAULT '',
    projects      TEXT NOT NULL DEFAULT '[]',
    decisions     TEXT NOT NULL DEFAULT '[]',
    feedback_sent_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);

-- FTS external content must expose the indexed columns; the entity lists
-- are combined into one column through this view.
CREATE VIEW IF NOT EXISTS memories_content AS
    SELECT id, title, summary, people || ' ' || organizations || ' ' || dates || ' ' || numbers || ' ' || projects || ' ' || decisions AS entities FROM memories;
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    title, summary, entities,
    content='memories_content', content_rowid='id', tokenize='porter unicode61'
);
CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, title, summary, entities)
    VALUES (new.id, new.title, new.summary, new.people || ' ' || new.organizations || ' ' || new.dates || ' ' || new.numbers || ' ' || new.projects || ' ' || new.decisions);
END;
CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, title, summary, entities)
    VALUES ('delete', old.id, old.title, old.summary, old.people || ' ' || old.organizations || ' ' || old.dates || ' ' || old.numbers || ' ' || old.projects || ' ' || old.decisions);
END;

-- People, organisations and projects as nodes with identity.
CREATE TABLE IF NOT EXISTS facts (
    id         INTEGER PRIMARY KEY,
    memory_id  INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    subject    TEXT NOT NULL,
    attribute  TEXT NOT NULL,
    value      TEXT NOT NULL,
    key        TEXT NOT NULL,             -- normalized subject|attribute
    as_of      INTEGER NOT NULL,          -- when it was seen
    origin     TEXT NOT NULL DEFAULT 'model',   -- model | user
    status     TEXT NOT NULL DEFAULT 'active',  -- active | retracted
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS facts_key ON facts(key);
CREATE INDEX IF NOT EXISTS facts_memory ON facts(memory_id);
CREATE VIRTUAL TABLE IF NOT EXISTS facts_fts USING fts5(
    subject, attribute, value, content='facts', content_rowid='id', tokenize='porter unicode61'
);
CREATE TRIGGER IF NOT EXISTS facts_ai AFTER INSERT ON facts BEGIN
    INSERT INTO facts_fts(rowid, subject, attribute, value) VALUES (new.id, new.subject, new.attribute, new.value);
END;
CREATE TRIGGER IF NOT EXISTS facts_ad AFTER DELETE ON facts BEGIN
    INSERT INTO facts_fts(facts_fts, rowid, subject, attribute, value) VALUES ('delete', old.id, old.subject, old.attribute, old.value);
END;
CREATE TABLE IF NOT EXISTS entities (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,            -- person | org | project
    name       TEXT NOT NULL,            -- display form, first seen
    normalized TEXT NOT NULL,
    first_seen INTEGER NOT NULL,
    last_seen  INTEGER NOT NULL,
    mentions   INTEGER NOT NULL DEFAULT 0,
    UNIQUE (kind, normalized)
);
CREATE TABLE IF NOT EXISTS memory_entities (
    memory_id INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    entity_id INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    PRIMARY KEY (memory_id, entity_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_memory_entities_entity ON memory_entities(entity_id);

-- Commitments and to-dos found in what was seen.
CREATE TABLE IF NOT EXISTS tasks (
    id         INTEGER PRIMARY KEY,
    memory_id  INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    text       TEXT NOT NULL,
    normalized TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'open',   -- open | done | dismissed
    created_at INTEGER NOT NULL,
    closed_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);

-- Generated daily briefings, one per local day.
CREATE TABLE IF NOT EXISTS recaps (
    day          TEXT PRIMARY KEY,
    text         TEXT NOT NULL,
    generated_at INTEGER NOT NULL,
    sources      TEXT NOT NULL DEFAULT '[]'
);

-- Where the user dragged nodes on the Board; absent = let the layout decide.
CREATE TABLE IF NOT EXISTS board_positions (
    node_key TEXT PRIMARY KEY,
    x REAL NOT NULL,
    y REAL NOT NULL
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS conversations (
    id         INTEGER PRIMARY KEY,
    title      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
    id              INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role            TEXT NOT NULL,          -- user | assistant
    content         TEXT NOT NULL,
    sources         TEXT NOT NULL DEFAULT '[]',
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id);
CREATE TABLE IF NOT EXISTS board_links (
    id         INTEGER PRIMARY KEY,
    from_key   TEXT NOT NULL,
    to_key     TEXT NOT NULL,
    label      TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    UNIQUE (from_key, to_key)
);
CREATE TABLE IF NOT EXISTS board_notes (
    node_key   TEXT PRIMARY KEY,
    text       TEXT NOT NULL,
    created_at INTEGER NOT NULL
) WITHOUT ROWID;

-- The day's signals: what matters, ranked, with why, and what the user did with each.
CREATE TABLE IF NOT EXISTS signals (
    id          INTEGER PRIMARY KEY,
    day         TEXT NOT NULL,
    kind        TEXT NOT NULL,              -- task | event | date | person | thread | memory
    ref_id      INTEGER NOT NULL DEFAULT 0,
    activity_id INTEGER NOT NULL DEFAULT 0,
    title       TEXT NOT NULL,
    reason      TEXT NOT NULL DEFAULT '',
    score       REAL NOT NULL DEFAULT 0,
    rank        INTEGER NOT NULL DEFAULT 0,
    state       TEXT NOT NULL DEFAULT 'open', -- open | pinned | done | noise
    features    TEXT NOT NULL DEFAULT '{}',
    created_at  INTEGER NOT NULL,
    UNIQUE (day, kind, ref_id, title)
);
CREATE INDEX IF NOT EXISTS idx_signals_day ON signals(day, rank);

-- How close each day's memories sat to the person's why.
CREATE TABLE IF NOT EXISTS alignment_days (
    day       TEXT PRIMARY KEY,
    score     REAL NOT NULL,
    memories  INTEGER NOT NULL,
    near      INTEGER NOT NULL
);

-- Recorded meetings; the transcript lives in the activity's snapshots.
CREATE TABLE IF NOT EXISTS meetings (
    id          INTEGER PRIMARY KEY,
    activity_id INTEGER NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    started_at  INTEGER NOT NULL,
    ended_at    INTEGER,
    status      TEXT NOT NULL DEFAULT 'recording',   -- recording | transcribing | done | failed
    detail      TEXT NOT NULL DEFAULT '',
    audio_dir   TEXT
);

-- Documents on this Mac and their text, in chunks.
CREATE TABLE IF NOT EXISTS files (
    id          INTEGER PRIMARY KEY,
    path        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    ext         TEXT NOT NULL,
    size        INTEGER NOT NULL,
    mtime       INTEGER NOT NULL,
    text_hash   TEXT NOT NULL DEFAULT '',
    indexed_at  INTEGER NOT NULL,
    chunk_count INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'ok'      -- ok | empty | error
);
CREATE TABLE IF NOT EXISTS file_chunks (
    id      INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    ord     INTEGER NOT NULL,
    text    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_file_chunks_file ON file_chunks(file_id);
CREATE VIRTUAL TABLE IF NOT EXISTS file_chunks_fts USING fts5(
    text, content='file_chunks', content_rowid='id', tokenize='porter unicode61'
);
CREATE TRIGGER IF NOT EXISTS file_chunks_ai AFTER INSERT ON file_chunks BEGIN
    INSERT INTO file_chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER IF NOT EXISTS file_chunks_au AFTER UPDATE OF text ON file_chunks BEGIN
    INSERT INTO file_chunks_fts(file_chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO file_chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER IF NOT EXISTS file_chunks_ad AFTER DELETE ON file_chunks BEGIN
    INSERT INTO file_chunks_fts(file_chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TABLE IF NOT EXISTS file_vectors (
    chunk_id INTEGER PRIMARY KEY REFERENCES file_chunks(id) ON DELETE CASCADE,
    vec      BLOB NOT NULL
);

-- Semantic search vectors, one per memory (unit-normalised f32 LE).
CREATE TABLE IF NOT EXISTS memory_vectors (
    memory_id INTEGER PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
    model     TEXT NOT NULL,
    vec       BLOB NOT NULL
);

-- How often each short line has appeared per source (site/app). Lines seen
-- repeatedly are interface chrome and are dropped from clean text.
CREATE TABLE IF NOT EXISTS source_lines (
    source    TEXT NOT NULL,
    line_hash TEXT NOT NULL,
    count     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (source, line_hash)
) WITHOUT ROWID;

CREATE VIRTUAL TABLE IF NOT EXISTS activities_fts USING fts5(
    app_name, window_title, url,
    content='activities', content_rowid='id', tokenize='porter unicode61'
);
CREATE TRIGGER IF NOT EXISTS activities_ai AFTER INSERT ON activities BEGIN
    INSERT INTO activities_fts(rowid, app_name, window_title, url)
    VALUES (new.id, new.app_name, new.window_title, new.url);
END;
CREATE TRIGGER IF NOT EXISTS activities_ad AFTER DELETE ON activities BEGIN
    INSERT INTO activities_fts(activities_fts, rowid, app_name, window_title, url)
    VALUES ('delete', old.id, old.app_name, old.window_title, old.url);
END;
CREATE TRIGGER IF NOT EXISTS activities_au AFTER UPDATE OF app_name, window_title, url ON activities BEGIN
    INSERT INTO activities_fts(activities_fts, rowid, app_name, window_title, url)
    VALUES ('delete', old.id, old.app_name, old.window_title, old.url);
    INSERT INTO activities_fts(rowid, app_name, window_title, url)
    VALUES (new.id, new.app_name, new.window_title, new.url);
END;

CREATE VIRTUAL TABLE IF NOT EXISTS snapshots_fts USING fts5(
    text,
    content='snapshots', content_rowid='id', tokenize='porter unicode61'
);
CREATE TRIGGER IF NOT EXISTS snapshots_ai AFTER INSERT ON snapshots BEGIN
    INSERT INTO snapshots_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER IF NOT EXISTS snapshots_ad AFTER DELETE ON snapshots BEGIN
    INSERT INTO snapshots_fts(snapshots_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER IF NOT EXISTS snapshots_au AFTER UPDATE OF text ON snapshots BEGIN
    INSERT INTO snapshots_fts(snapshots_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO snapshots_fts(rowid, text) VALUES (new.id, new.text);
END;
"#;

/// Bump when the model's output schema changes in a way worth remaking
/// existing memories for.
pub const MEMORY_SCHEMA_VERSION: i64 = 6;

/// Snippet highlight markers. Control characters never occur in captured
/// text, so the UI can split on them safely.
pub const HL_START: char = '\u{1}';
pub const HL_END: char = '\u{2}';

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub interval_secs: u64,
    pub idle_minutes: u64,
    /// User-added apps to skip. System prompts and password managers are
    /// always skipped (see privacy.rs) and are not listed here.
    pub excluded_apps: Vec<String>,
    pub excluded_url_patterns: Vec<String>,
    pub read_browser_text: bool,
    pub exclude_messaging: bool,
    pub exclude_email: bool,
    pub onboarding_done: bool,
    /// Replace email addresses and phone numbers before storing.
    pub redact_contacts: bool,
    /// Local model used by the memory engine.
    pub model: String,
    /// Build that last held the Accessibility grant (see permissions.rs).
    pub trusted_build: String,
    /// Folders whose documents are indexed. Empty = the defaults.
    pub index_folders: Vec<String>,
    pub index_files: bool,
    /// Keep meeting audio after transcription (default: delete).
    pub keep_audio: bool,
    /// Write briefings, meeting summaries and notes as Markdown here (e.g. an
    /// Obsidian vault). Empty = off.
    pub markdown_folder: String,
    /// Send thumbs (with the screen text they judged) to Lane to train Rabbit.
    /// Off by default; the one deliberate network use besides backups.
    pub contribute_labels: bool,
    pub tester_token: String,
    pub labels_last_sent_at: i64,
    /// Encrypted backups: on once a passphrase is set.
    pub backup_enabled: bool,
    pub backup_folder: String,
    pub last_backup_at: i64,
    /// Speech language for transcription: "auto", "en", "hi", …
    pub meeting_language: String,
    /// Read the Mac's Calendar for meeting prep (asks for the Calendars permission).
    pub calendar_enabled: bool,
    /// Notion: token lives in the Keychain; pages are imported into the file index.
    pub notion_enabled: bool,
    /// Page under which Lane creates exported notes (id or URL).
    pub notion_parent_page: String,
    pub notion_last_sync_at: i64,
    /// Days to keep raw screen text after a memory was made (0 = forever).
    /// Memories, facts, tasks, notes and meeting transcripts are kept.
    pub raw_retention_days: i64,
    /// A notification 15 minutes before each calendar event with a Prepare hint.
    pub meeting_notifications: bool,
    /// Apple Mail (inbox + sent) through Mail's automation interface.
    pub mail_enabled: bool,
    /// Make memories of mail, not only index it (off: index only).
    pub mail_memories: bool,
    /// Apple Contacts for first names and nicknames.
    pub contacts_enabled: bool,
    pub contacts_last_sync_at: i64,
    /// Keep what the user copies (opt-in; secrets are never stored).
    pub clipboard_enabled: bool,
    /// Daily briefing notifications: "HH:MM" local, empty = off.
    pub morning_brief_at: String,
    pub evening_brief_at: String,
    /// The strip under the notch: reminders, briefs, live meeting help.
    pub notch_enabled: bool,
    /// While typing: repeats, and contradictions with your own facts (opt-in).
    pub thought_check_enabled: bool,
    /// Tell the other side's speakers apart in meeting transcripts (downloads a
    /// speaker model once, ~60 MB; opt-in).
    pub diarize_enabled: bool,
    /// Keep a small picture of the front window with each memory (needs
    /// Screen Recording; opt-in).
    pub screenshots_enabled: bool,
    /// Start recording by itself when a meeting app or site is in front, and
    /// stop when it has gone (opt-in). Off: a nudge to press Record instead.
    pub auto_listen_calls: bool,
    /// Where the notch tab lives: top-center, top-left, top-right,
    /// bottom-center, left, right.
    pub notch_position: String,
    /// The user picked the position (onboarding or Settings); an installed
    /// notch app then never moves it.
    pub notch_position_chosen: bool,
    /// The person's why, in their words (the centre of the circle).
    /// What the person asked to be called, set during setup. Empty means
    /// fall back to the macOS account name.
    pub display_name: String,
    pub purpose_why: String,
    /// Their principles, one line each (how).
    pub purpose_how: Vec<String>,
    /// Learned weights for the signal scorer, by feature name.
    pub signal_weights: std::collections::HashMap<String, f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_secs: 5,
            idle_minutes: 5,
            excluded_apps: Vec::new(),
            excluded_url_patterns: Vec::new(),
            read_browser_text: true,
            exclude_messaging: true,
            exclude_email: false,
            onboarding_done: false,
            redact_contacts: true,
            model: crate::runtime::default_model().file.into(),
            trusted_build: String::new(),
            index_folders: Vec::new(),
            index_files: true,
            keep_audio: false,
            markdown_folder: String::new(),
            contribute_labels: false,
            tester_token: String::new(),
            labels_last_sent_at: 0,
            backup_enabled: false,
            backup_folder: String::new(),
            last_backup_at: 0,
            meeting_language: "auto".into(),
            calendar_enabled: false,
            notion_enabled: false,
            notion_parent_page: String::new(),
            notion_last_sync_at: 0,
            raw_retention_days: 30,
            meeting_notifications: true,
            mail_enabled: false,
            mail_memories: false,
            contacts_enabled: false,
            contacts_last_sync_at: 0,
            clipboard_enabled: false,
            morning_brief_at: "07:00".into(),
            evening_brief_at: "19:00".into(),
            notch_enabled: true,
            thought_check_enabled: false,
            diarize_enabled: false,
            screenshots_enabled: false,
            auto_listen_calls: false,
            notch_position: "top-center".into(),
            notch_position_chosen: false,
            display_name: String::new(),
            purpose_why: String::new(),
            purpose_how: Vec::new(),
            signal_weights: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySummary {
    pub id: i64,
    pub app_name: String,
    pub app_path: Option<String>,
    pub window_title: String,
    pub url: Option<String>,
    pub started_at: i64,
    pub ended_at: i64,
    pub snapshot_count: i64,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: i64,
    pub captured_at: i64,
    /// Cleaned text: what the UI shows and search indexes.
    pub text: String,
    /// The accessibility dump the clean text was derived from.
    pub raw: String,
    /// A small JPEG of the window at the time, when screenshots are on.
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingActivity {
    pub id: i64,
    pub app_name: String,
    pub window_title: String,
    pub url: Option<String>,
    pub started_at: i64,
    pub ended_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Upcoming {
    pub when: i64,
    /// The date as it was written.
    pub label: String,
    pub title: String,
    pub activity_id: i64,
    pub memory_id: i64,
    /// What the date is attached to (a fact's subject/attribute, or "mentioned").
    pub about: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gap {
    pub kind: String,
    pub text: String,
    pub entity_id: Option<i64>,
    pub question: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: i64,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub sources: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardLink {
    pub id: i64,
    pub from_key: String,
    pub to_key: String,
    pub label: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardNote {
    pub key: String,
    pub text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ForgetReport {
    pub entities: usize,
    pub memories: usize,
    pub facts: usize,
    pub tasks: usize,
    pub snapshots: usize,
    pub recaps: usize,
    pub files: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Explore {
    /// Minutes in the last 30 days by what the app or site is for.
    pub by_category: Vec<(String, i64)>,
    /// The apps and sites you spent most time in, with their category.
    pub top_places: Vec<(String, String, i64)>,
    pub memories_per_day: Vec<(String, i64)>,
    pub minutes_per_day: Vec<(String, i64)>,
    pub by_kind: Vec<(String, i64)>,
    pub top_people: Vec<Entity>,
    pub top_orgs: Vec<Entity>,
    pub top_projects: Vec<Entity>,
    pub facts: i64,
    pub conflicts: i64,
    pub tasks_open: i64,
    pub tasks_done: i64,
    pub files: i64,
    pub meetings: i64,
    pub memories: i64,
    pub kept: i64,
    pub pinned: i64,
    pub edited: i64,
}

/// A specific value the text states: subject, attribute, value.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewFact {
    pub subject: String,
    pub attribute: String,
    pub value: String,
    /// Whose fact it is: "mine" (the user's own account, work, figure),
    /// "theirs" (someone else's), "unknown".
    #[serde(default = "unknown")]
    pub owner: String,
    /// How firm it is: "stated", "proposed", "agreed", "asked".
    #[serde(default = "stated")]
    pub stance: String,
}

fn unknown() -> String { "unknown".into() }
fn stated() -> String { "stated".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactRef {
    pub fact_id: i64,
    pub memory_id: i64,
    pub value: String,
    pub as_of: i64,
    pub title: String,
    pub activity_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub id: i64,
    pub memory_id: i64,
    pub subject: String,
    pub attribute: String,
    pub value: String,
    pub as_of: i64,
    pub origin: String,
    pub owner: String,
    pub stance: String,
    /// Other active facts with the same subject and attribute but a
    /// different value, newest first.
    pub conflicts: Vec<FactRef>,
}

/// What the model produced for one activity, after verification.
#[derive(Debug, Clone, Default)]
pub struct NewMemory {
    pub facts: Vec<NewFact>,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub people: Vec<String>,
    pub organizations: Vec<String>,
    pub dates: Vec<String>,
    pub numbers: Vec<String>,
    pub projects: Vec<String>,
    pub tasks: Vec<String>,
    pub decisions: Vec<String>,
    pub keep: bool,
    pub confidence: f64,
    pub dropped: i64,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub mentions: i64,
    pub first_seen: i64,
    pub last_seen: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub memory_id: i64,
    pub text: String,
    pub status: String,
    pub created_at: i64,
    pub title: String,
    pub app_name: String,
    pub started_at: i64,
    pub activity_id: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: i64,
    pub activity_id: i64,
    pub title: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub status: String,
    pub detail: String,
    pub memory_title: Option<String>,
    pub memory_summary: Option<String>,
    /// Structured notes written after the transcript (Markdown); empty until then.
    pub notes: String,
    /// Names seen on screen in the meeting app while recording.
    pub attendees: Vec<String>,
    /// The recording was kept (Settings → Keep audio) and can be played.
    pub has_audio: bool,
    /// Speaker labels in the transcript ("Speaker 1"…) and the names given to them.
    pub speakers: Vec<Speaker>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Speaker {
    pub label: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signal {
    pub id: i64,
    pub day: String,
    pub kind: String,
    pub ref_id: i64,
    pub activity_id: i64,
    pub title: String,
    pub reason: String,
    pub score: f64,
    pub rank: i64,
    pub state: String,
    pub features: std::collections::HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    pub file_id: i64,
    pub path: String,
    pub name: String,
    pub ext: String,
    pub mtime: i64,
    pub size: i64,
    pub snippet: String,
    pub chunk_id: i64,
    /// Where to open it when it is not a file on disk (connector items).
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStats {
    pub files: i64,
    pub chunks: i64,
    pub unembedded: i64,
    pub last_indexed_at: i64,
    /// Extension and how many indexed files carry it, biggest first.
    pub by_ext: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub source: i64,
    pub target: i64,
    pub weight: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub nodes: Vec<Entity>,
    pub edges: Vec<GraphEdge>,
}

/// "14 October", "14 Oct 2026", "October 14", "Oct 14, 2026", "14/10/2026",
/// "2026-10-14", "14-10-2026" → local midnight (ms). No year = the next
/// occurrence from `today`. Anything else → None.
pub fn parse_written_date(text: &str, today: i64) -> Option<i64> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let t = text.trim().to_lowercase();
    let toks: Vec<String> = t.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.trim_end_matches("st").trim_end_matches("nd").trim_end_matches("rd").trim_end_matches("th").to_string()).collect();
    let toks: Vec<&str> = toks.iter().map(|x| x.as_str()).collect();
    let month_of = |w: &str| MONTHS.iter().position(|m| w.starts_with(m) && w.len() >= 3).map(|i| i as i64 + 1);
    let (y0, m0, d0) = {
        let d = crate::engine::day_of(today);
        let mut it = d.split('-').map(|x| x.parse::<i64>().unwrap_or(1));
        (it.next().unwrap_or(2026), it.next().unwrap_or(1), it.next().unwrap_or(1))
    };
    let mut year: Option<i64> = None;
    let mut month: Option<i64> = None;
    let mut day: Option<i64> = None;
    // ISO / numeric forms.
    if toks.len() == 3 && toks.iter().all(|x| x.chars().all(|c| c.is_ascii_digit())) {
        let n: Vec<i64> = toks.iter().map(|x| x.parse().unwrap_or(0)).collect();
        if n[0] > 1900 {
            (year, month, day) = (Some(n[0]), Some(n[1]), Some(n[2]));
        } else if n[2] > 1900 {
            (year, month, day) = (Some(n[2]), Some(n[1]), Some(n[0]));
        } else {
            return None;
        }
    } else {
        for w in &toks {
            if let Some(m) = month_of(w) {
                month = Some(m);
            } else if let Ok(n) = w.parse::<i64>() {
                if n > 1900 && n < 2200 {
                    year = Some(n);
                } else if (1..=31).contains(&n) && day.is_none() {
                    day = Some(n);
                }
            }
        }
    }
    let (m, d) = (month?, day?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = year.unwrap_or(if (m, d) < (m0, d0 - 1) { y0 + 1 } else { y0 });
    crate::engine::local_midnight(y, m, d)
}

fn contains_ci(hay: &str, needle: &str) -> bool {
    hay.to_lowercase().contains(&needle.to_lowercase())
}

/// Case-insensitive replace that keeps the original text's characters.
pub fn replace_ci(hay: &str, needle: &str, with: &str) -> String {
    if needle.is_empty() {
        return hay.to_string();
    }
    let lower = hay.to_lowercase();
    let nl = needle.to_lowercase();
    // Byte offsets in `lower` match `hay` only when lowercasing keeps lengths;
    // work on char indices to stay safe with non-ASCII.
    let hay_chars: Vec<char> = hay.chars().collect();
    let low_chars: Vec<char> = lower.chars().collect();
    let n_chars: Vec<char> = nl.chars().collect();
    if low_chars.len() != hay_chars.len() || n_chars.is_empty() {
        return hay.replace(needle, with);
    }
    let mut out = String::new();
    let mut i = 0;
    while i < hay_chars.len() {
        if i + n_chars.len() <= low_chars.len() && low_chars[i..i + n_chars.len()] == n_chars[..] {
            out.push_str(with);
            i += n_chars.len();
        } else {
            out.push(hay_chars[i]);
            i += 1;
        }
    }
    out
}

/// Subject and attribute, lowercased and squeezed, so "Vatsalya budget"
/// and "vatsalya  Budget" are the same fact.
pub fn fact_key(subject: &str, attribute: &str) -> String {
    let squeeze = |s: &str| s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect::<Vec<_>>().join(" ");
    format!("{}|{}", squeeze(subject), squeeze(attribute))
}

/// Identity for an entity: case, spacing and trailing punctuation don't
/// make a different person.
pub fn normalize_entity(name: &str) -> String {
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    n.trim_matches(|c: char| c == '.' || c == ',' || c == ';' || c == ':' || c == '"' || c == '\'').to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCard {
    pub id: i64,
    pub activity_id: i64,
    pub created_at: i64,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub people: Vec<String>,
    pub organizations: Vec<String>,
    pub dates: Vec<String>,
    pub numbers: Vec<String>,
    pub projects: Vec<String>,
    pub decisions: Vec<String>,
    pub keep: bool,
    pub confidence: f64,
    pub dropped: i64,
    pub model: String,
    pub feedback: Option<String>,
    pub app_name: String,
    pub window_title: String,
    pub url: Option<String>,
    pub started_at: i64,
    pub ended_at: i64,
    pub group_key: String,
    /// Sessions merged into this card (same page/document, close in time).
    pub sessions: i64,
    pub total_ms: i64,
    /// Memory ids in the group, latest first; thumbs apply to all of them.
    pub ids: Vec<i64>,
    /// Pinned by the user: never remade, ranked first.
    pub pinned: bool,
    pub edited_at: Option<i64>,
    /// Filled by `attach_facts`; empty otherwise.
    pub facts: Vec<Fact>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCounts {
    pub pending: i64,
    pub memories: i64,
    pub kept: i64,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecleanStats {
    pub snapshots: i64,
    pub raw_chars: i64,
    pub clean_chars: i64,
    pub removed_empty: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDetail {
    pub activity: ActivitySummary,
    pub snapshots: Vec<Snapshot>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub activity: ActivitySummary,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub activities: i64,
    pub snapshots: i64,
    pub activities_today: i64,
    pub db_size_bytes: u64,
}

pub struct Store {
    conn: Connection,
    path: Option<std::path::PathBuf>,
    /// Memory vectors, 8-bit, in memory: 100K memories × 768 dims is 77 MB
    /// and a query scans it in tens of milliseconds. Built on first use,
    /// appended on every new vector, rebuilt after a delete.
    vindex: std::cell::RefCell<Option<QuantIndex>>,
    findex: std::cell::RefCell<Option<QuantIndex>>,
}

/// A flat 8-bit vector index with per-vector scale. Brute force, exact
/// ranking within quantisation error, no build step worth speaking of.
pub struct QuantIndex {
    dim: usize,
    ids: Vec<i64>,
    q: Vec<i8>,
    scale: Vec<f32>,
}

impl QuantIndex {
    fn new(dim: usize) -> Self {
        Self { dim, ids: Vec::new(), q: Vec::new(), scale: Vec::new() }
    }
    fn push(&mut self, id: i64, v: &[f32]) {
        if v.len() != self.dim {
            return;
        }
        let max = v.iter().fold(0f32, |m, x| m.max(x.abs()));
        let scale = if max > 0.0 { max / 127.0 } else { 1.0 };
        self.ids.push(id);
        self.scale.push(scale);
        self.q.extend(v.iter().map(|x| (x / scale).round().clamp(-127.0, 127.0) as i8));
    }
    /// Top `limit` ids by dot product with a unit query.
    fn search(&self, query: &[f32], limit: usize) -> Vec<(i64, f32)> {
        if query.len() != self.dim || self.ids.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(i64, f32)> = Vec::with_capacity(self.ids.len());
        for (i, id) in self.ids.iter().enumerate() {
            let row = &self.q[i * self.dim..(i + 1) * self.dim];
            let mut acc = 0f32;
            for (a, b) in row.iter().zip(query.iter()) {
                acc += *a as f32 * b;
            }
            scored.push((*id, acc * self.scale[i]));
        }
        // Partial sort: only the top `limit` need ordering.
        let k = limit.min(scored.len());
        scored.select_nth_unstable_by(k.saturating_sub(1).max(0), |a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }
    pub fn len(&self) -> usize {
        self.ids.len()
    }
}

fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

pub fn text_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(text.as_bytes()))
}

impl Store {
    /// Open the encrypted vault, encrypting a pre-vault plaintext database
    /// on the way if one is found.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let key = crate::vault::vault_key().map_err(rusqlite::Error::InvalidParameterName)?;
        let bak = path.with_extension("db.pre-vault");
        if path.is_file() && crate::vault::is_plaintext_db(path) {
            // Keep the plaintext until the vault is proven readable.
            std::fs::copy(path, &bak).map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
            crate::vault::encrypt_in_place(path, &key).map_err(rusqlite::Error::InvalidParameterName)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(&format!("PRAGMA key = {};", crate::vault::key_pragma(&key)))?;
        if bak.is_file() {
            let ok = conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).map(|v| v == "ok").unwrap_or(false)
                && conn.execute_batch("INSERT INTO snapshots_fts(snapshots_fts) VALUES ('integrity-check'); INSERT INTO memories_fts(memories_fts) VALUES ('integrity-check');").is_ok();
            if ok {
                let _ = std::fs::remove_file(&bak);
                log::info!("vault: integrity verified, plaintext copy removed");
            } else {
                drop(conn);
                let _ = std::fs::rename(&bak, path);
                log::error!("vault: encrypted copy failed verification; restored plaintext");
                return Err(rusqlite::Error::InvalidParameterName("encrypted database failed verification".into()));
            }
        }
        conn.execute_batch(SCHEMA)?;
        Self::migrate(&conn)?;
        Ok(Self { conn, path: Some(path.to_path_buf()), vindex: std::cell::RefCell::new(None), findex: std::cell::RefCell::new(None) })
    }

    /// Open the vault read-only with the Keychain key (the MCP server).
    /// No schema changes: if the app has not migrated yet, queries fail
    /// rather than two processes migrating at once.
    pub fn open_readonly(path: &Path) -> rusqlite::Result<Self> {
        let key = crate::vault::keychain_get(crate::vault::VAULT_SERVICE).ok_or_else(|| rusqlite::Error::InvalidParameterName("no vault key in the Keychain".into()))?;
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        conn.execute_batch(&format!("PRAGMA key = {};", crate::vault::key_pragma(&key)))?;
        conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0))?;
        Ok(Self { conn, path: Some(path.to_path_buf()), vindex: std::cell::RefCell::new(None), findex: std::cell::RefCell::new(None) })
    }

    /// Open a plaintext database (tests, exported copies).
    pub fn open_plain(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Self::migrate(&conn)?;
        Ok(Self { conn, path: Some(path.to_path_buf()), vindex: std::cell::RefCell::new(None), findex: std::cell::RefCell::new(None) })
    }

    pub fn export_plaintext(&self, dest: &Path) -> Result<(), String> {
        crate::vault::export_plaintext(&self.conn, dest)
    }

    pub fn backup(&self, folder: &Path, passphrase: &str, stamp: &str) -> Result<std::path::PathBuf, String> {
        crate::backup::write(&self.conn, folder, passphrase, stamp)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Self::migrate(&conn)?;
        Ok(Self { conn, path: None, vindex: std::cell::RefCell::new(None), findex: std::cell::RefCell::new(None) })
    }

    /// Additive migrations for databases created by earlier builds.
    fn migrate(conn: &Connection) -> rusqlite::Result<()> {
        let meeting_cols: Vec<String> = conn.prepare("PRAGMA table_info(meetings)")?.query_map([], |r| r.get::<_, String>(1))?.filter_map(Result::ok).collect();
        if !meeting_cols.iter().any(|c| c == "notes") {
            conn.execute_batch("ALTER TABLE meetings ADD COLUMN notes TEXT NOT NULL DEFAULT ''")?;
        }
        if !meeting_cols.iter().any(|c| c == "attendees") {
            conn.execute_batch("ALTER TABLE meetings ADD COLUMN attendees TEXT NOT NULL DEFAULT '[]'")?;
        }
        if !meeting_cols.iter().any(|c| c == "speakers") {
            conn.execute_batch("ALTER TABLE meetings ADD COLUMN speakers TEXT NOT NULL DEFAULT '[]'")?;
        }
        let snap_cols: Vec<String> = conn.prepare("PRAGMA table_info(snapshots)")?.query_map([], |r| r.get::<_, String>(1))?.filter_map(Result::ok).collect();
        if !snap_cols.iter().any(|c| c == "image") {
            conn.execute_batch("ALTER TABLE snapshots ADD COLUMN image TEXT")?;
        }
        let has_raw: bool = conn
            .prepare("PRAGMA table_info(snapshots)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|c| c == "raw_text");
        if !has_raw {
            conn.execute_batch("ALTER TABLE snapshots ADD COLUMN raw_text TEXT")?;
        }
        let has_status: bool = conn
            .prepare("PRAGMA table_info(activities)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|c| c == "memory_status");
        if !has_status {
            conn.execute_batch("ALTER TABLE activities ADD COLUMN memory_status TEXT NOT NULL DEFAULT 'pending'")?;
        }
        let has_group: bool = conn
            .prepare("PRAGMA table_info(memories)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|c| c == "group_key");
        if !has_group {
            conn.execute_batch("ALTER TABLE memories ADD COLUMN group_key TEXT NOT NULL DEFAULT ''")?;
        }
        let has_sent: bool = conn
            .prepare("PRAGMA table_info(memories)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|c| c == "feedback_sent_at");
        if !has_sent {
            conn.execute_batch("ALTER TABLE memories ADD COLUMN feedback_sent_at INTEGER")?;
        }
        let has_projects: bool = conn
            .prepare("PRAGMA table_info(memories)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|c| c == "projects");
        if !has_projects {
            conn.execute_batch("ALTER TABLE memories ADD COLUMN projects TEXT NOT NULL DEFAULT '[]'")?;
        }
        let has_decisions: bool = conn
            .prepare("PRAGMA table_info(memories)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|c| c == "decisions");
        if !has_decisions {
            conn.execute_batch(
                "ALTER TABLE memories ADD COLUMN decisions TEXT NOT NULL DEFAULT '[]';
                 DROP VIEW IF EXISTS memories_content;
                 CREATE VIEW memories_content AS
                     SELECT id, title, summary, people || ' ' || organizations || ' ' || dates || ' ' || numbers || ' ' || projects || ' ' || decisions AS entities FROM memories;
                 DROP TRIGGER IF EXISTS memories_ai; DROP TRIGGER IF EXISTS memories_ad;
                 CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
                     INSERT INTO memories_fts(rowid, title, summary, entities)
                     VALUES (new.id, new.title, new.summary, new.people || ' ' || new.organizations || ' ' || new.dates || ' ' || new.numbers || ' ' || new.projects || ' ' || new.decisions);
                 END;
                 CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
                     INSERT INTO memories_fts(memories_fts, rowid, title, summary, entities)
                     VALUES ('delete', old.id, old.title, old.summary, old.people || ' ' || old.organizations || ' ' || old.dates || ' ' || old.numbers || ' ' || old.projects || ' ' || old.decisions);
                 END;
                 INSERT INTO memories_fts(memories_fts) VALUES ('rebuild');",
            )?;
        }
        // Databases from before the memories_content view still have the
        // search index reading the memories table, which lacks `entities`:
        // rebuilds fail and search breaks. Recreate it against the view.
        let fts_sql: Option<String> = conn
            .query_row("SELECT sql FROM sqlite_master WHERE name = 'memories_fts'", [], |r| r.get(0))
            .optional()?;
        if fts_sql.map_or(false, |sql| !sql.contains("memories_content")) {
            conn.execute_batch(
                "DROP TABLE memories_fts;
                 CREATE VIRTUAL TABLE memories_fts USING fts5(
                     title, summary, entities,
                     content='memories_content', content_rowid='id', tokenize='porter unicode61'
                 );
                 INSERT INTO memories_fts(memories_fts) VALUES ('rebuild');",
            )?;
            log::info!("store: rebuilt the memory search index against the content view");
        }
        let has_owner: bool = conn
            .prepare("PRAGMA table_info(facts)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|c| c == "owner");
        if !has_owner {
            conn.execute_batch("ALTER TABLE facts ADD COLUMN owner TEXT NOT NULL DEFAULT 'unknown'; ALTER TABLE facts ADD COLUMN stance TEXT NOT NULL DEFAULT 'stated'")?;
        }
        let has_pinned: bool = conn
            .prepare("PRAGMA table_info(memories)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|c| c == "pinned");
        if !has_pinned {
            conn.execute_batch("ALTER TABLE memories ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0; ALTER TABLE memories ADD COLUMN edited_at INTEGER")?;
        }
        // Edits must reach the search index: an UPDATE trigger (older
        // databases only had insert/delete).
        conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE OF title, summary, people, organizations, dates, numbers, projects, decisions ON memories BEGIN
                 INSERT INTO memories_fts(memories_fts, rowid, title, summary, entities)
                 VALUES ('delete', old.id, old.title, old.summary, old.people || ' ' || old.organizations || ' ' || old.dates || ' ' || old.numbers || ' ' || old.projects || ' ' || old.decisions);
                 INSERT INTO memories_fts(rowid, title, summary, entities)
                 VALUES (new.id, new.title, new.summary, new.people || ' ' || new.organizations || ' ' || new.dates || ' ' || new.numbers || ' ' || new.projects || ' ' || new.decisions);
             END;",
        )?;
        let has_url: bool = conn
            .prepare("PRAGMA table_info(files)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|c| c == "url");
        if !has_url {
            conn.execute_batch("ALTER TABLE files ADD COLUMN url TEXT NOT NULL DEFAULT ''")?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS entity_aliases (
                 alias      TEXT PRIMARY KEY,           -- normalized
                 entity_id  INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                 shown      TEXT NOT NULL
             );",
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS connector_runs (
                 id       TEXT PRIMARY KEY,
                 last_run INTEGER NOT NULL DEFAULT 0,
                 items    INTEGER NOT NULL DEFAULT 0,
                 error    TEXT NOT NULL DEFAULT ''
             );",
        )?;
        let has_profile: bool = conn
            .prepare("PRAGMA table_info(entities)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|c| c == "profile");
        if !has_profile {
            conn.execute_batch("ALTER TABLE entities ADD COLUMN profile TEXT; ALTER TABLE entities ADD COLUMN profile_at INTEGER NOT NULL DEFAULT 0")?;
        }
        // The model's output schema grew (projects, tasks): memories made
        // with the old schema are remade in the background. Thumbs survive.
        let version: i64 = conn
            .query_row("SELECT value FROM settings WHERE key = 'memory_schema'", [], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        if version < MEMORY_SCHEMA_VERSION {
            // Memories the user edited or pinned are theirs: never remade.
            conn.execute_batch(
                "UPDATE activities SET memory_status = 'pending' WHERE memory_status = 'done'
                 AND id NOT IN (SELECT activity_id FROM memories WHERE pinned = 1 OR edited_at IS NOT NULL)",
            )?;
            conn.execute(
                "INSERT INTO settings(key, value) VALUES ('memory_schema', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![MEMORY_SCHEMA_VERSION.to_string()],
            )?;
        }
        // Activities that failed (usually a truncated or malformed answer)
        // get one more try per launch.
        conn.execute_batch("UPDATE activities SET memory_status = 'pending' WHERE memory_status = 'failed'")?;
        // Backfill for memories made before grouping existed.
        let rows: Vec<(i64, String, String, Option<String>)> = conn
            .prepare("SELECT m.id, a.app_name, a.window_title, a.url FROM memories m JOIN activities a ON a.id = m.activity_id WHERE m.group_key = ''")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, app, title, url) in rows {
            conn.execute("UPDATE memories SET group_key = ?2 WHERE id = ?1", params![id, clean::group_key(&app, &title, url.as_deref())])?;
        }
        Ok(())
    }

    /// True when snapshots exist that were stored before text cleanup.
    pub fn needs_reclean(&self) -> rusqlite::Result<bool> {
        self.conn.query_row("SELECT EXISTS(SELECT 1 FROM snapshots WHERE raw_text IS NULL)", [], |r| r.get(0))
    }

    // ── Settings ──────────────────────────────────────────────────────────

    pub fn settings(&self) -> Settings {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> rusqlite::Result<()> {
        let json = serde_json::to_string(settings).expect("settings serialize");
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES ('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    // ── Writes (called by the capture loop) ──────────────────────────────

    pub fn start_activity(
        &self,
        app_name: &str,
        app_path: Option<&str>,
        window_title: &str,
        url: Option<&str>,
        at: i64,
    ) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO activities(app_name, app_path, window_title, url, started_at, ended_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![app_name, app_path, window_title, url, at],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn extend_activity(&self, id: i64, ended_at: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE activities SET ended_at = MAX(ended_at, ?2) WHERE id = ?1",
            params![id, ended_at],
        )?;
        Ok(())
    }

    /// Store a raw capture for an activity. Returns None when nothing
    /// content-bearing remains after cleanup, or when the clean text equals
    /// the activity's previous snapshot.
    pub fn add_snapshot(&self, activity_id: i64, raw: &str, at: i64) -> rusqlite::Result<Option<i64>> {
        let source: String = self
            .conn
            .query_row("SELECT app_name, url FROM activities WHERE id = ?1", params![activity_id], |r| {
                Ok(clean::source_key(&r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?.as_deref()))
            })?;
        let tx = self.conn.unchecked_transaction()?;
        let clean_text = Self::clean_with_stats(&tx, &source, raw)?;
        let hash = text_hash(&clean_text);
        let previous: Option<String> = tx
            .query_row(
                "SELECT text_hash FROM snapshots WHERE activity_id = ?1 ORDER BY captured_at DESC, id DESC LIMIT 1",
                params![activity_id],
                |r| r.get(0),
            )
            .optional()?;
        if clean_text.is_empty() || previous.as_deref() == Some(hash.as_str()) {
            tx.commit()?;
            return Ok(None);
        }
        tx.execute(
            "INSERT INTO snapshots(activity_id, captured_at, text, raw_text, text_hash) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![activity_id, at, clean_text, raw, hash],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "UPDATE activities SET snapshot_count = snapshot_count + 1, memory_status = 'pending' WHERE id = ?1",
            params![activity_id],
        )?;
        tx.commit()?;
        Ok(Some(id))
    }

    /// Clean `raw` against the source's line statistics, then record its
    /// lines so later snapshots learn what this source's chrome looks like.
    fn clean_with_stats(conn: &Connection, source: &str, raw: &str) -> rusqlite::Result<String> {
        let mut seen = conn.prepare_cached("SELECT count FROM source_lines WHERE source = ?1 AND line_hash = ?2")?;
        let cleaned = clean::clean_text(raw, |line| {
            seen.query_row(params![source, text_hash(line)], |r| r.get::<_, u32>(0)).unwrap_or(0)
        });
        let mut bump = conn.prepare_cached(
            "INSERT INTO source_lines(source, line_hash, count) VALUES (?1, ?2, 1)
             ON CONFLICT(source, line_hash) DO UPDATE SET count = count + 1",
        )?;
        for line in clean::distinct_lines(raw) {
            if line.chars().count() <= clean::BOILERPLATE_MAX_LEN {
                bump.execute(params![source, text_hash(&line)])?;
            }
        }
        Ok(cleaned)
    }

    /// Re-run cleanup over every stored snapshot, oldest first, rebuilding
    /// the source statistics from scratch. Used after upgrades and to
    /// measure rule changes on real captures.
    pub fn reclean_all(&self) -> rusqlite::Result<RecleanStats> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch("DELETE FROM source_lines")?;
        let rows: Vec<(i64, i64, String, String)> = tx
            .prepare(
                "SELECT s.id, s.activity_id, COALESCE(s.raw_text, s.text), a.app_name || char(0) || COALESCE(a.url, '')
                 FROM snapshots s JOIN activities a ON a.id = s.activity_id ORDER BY s.captured_at, s.id",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut stats = RecleanStats::default();
        let mut last_hash: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
        for (id, activity_id, raw, app_url) in rows {
            let (app, url) = app_url.split_once('\0').unwrap_or((&app_url, ""));
            let source = clean::source_key(app, (!url.is_empty()).then_some(url));
            let cleaned = if clean::is_title_only(app, None) { String::new() } else { Self::clean_with_stats(&tx, &source, &raw)? };
            let hash = text_hash(&cleaned);
            stats.snapshots += 1;
            stats.raw_chars += raw.chars().count() as i64;
            if cleaned.is_empty() || last_hash.get(&activity_id) == Some(&hash) {
                tx.execute("DELETE FROM snapshots WHERE id = ?1", params![id])?;
                tx.execute("UPDATE activities SET snapshot_count = snapshot_count - 1 WHERE id = ?1", params![activity_id])?;
                stats.removed_empty += 1;
                continue;
            }
            stats.clean_chars += cleaned.chars().count() as i64;
            tx.execute(
                "UPDATE snapshots SET text = ?2, raw_text = ?3, text_hash = ?4 WHERE id = ?1",
                params![id, cleaned, raw, hash],
            )?;
            last_hash.insert(activity_id, hash);
        }
        tx.execute_batch("INSERT INTO snapshots_fts(snapshots_fts) VALUES ('rebuild')")?;
        tx.commit()?;
        Ok(stats)
    }

    /// An activity with no text and a trivially short dwell is noise
    /// (alt-tab flicker). Dropped when it closes.
    pub fn discard_if_trivial(&self, id: i64, min_duration_ms: i64) -> rusqlite::Result<bool> {
        let n = self.conn.execute(
            "DELETE FROM activities WHERE id = ?1 AND snapshot_count = 0 AND ended_at - started_at < ?2",
            params![id, min_duration_ms],
        )?;
        Ok(n > 0)
    }

    // ── Reads ─────────────────────────────────────────────────────────────

    const SUMMARY_COLS: &'static str = "a.id, a.app_name, a.app_path, a.window_title, a.url, a.started_at, a.ended_at, a.snapshot_count,
        (SELECT substr(s.text, 1, 240) FROM snapshots s WHERE s.activity_id = a.id ORDER BY s.captured_at DESC LIMIT 1)";

    fn row_to_summary(r: &rusqlite::Row) -> rusqlite::Result<ActivitySummary> {
        Ok(ActivitySummary {
            id: r.get(0)?,
            app_name: r.get(1)?,
            app_path: r.get(2)?,
            window_title: r.get(3)?,
            url: r.get(4)?,
            started_at: r.get(5)?,
            ended_at: r.get(6)?,
            snapshot_count: r.get(7)?,
            preview: r.get(8)?,
        })
    }

    pub fn list_activities(&self, before: Option<i64>, limit: u32) -> rusqlite::Result<Vec<ActivitySummary>> {
        let sql = format!(
            "SELECT {} FROM activities a WHERE a.started_at < ?1 ORDER BY a.started_at DESC LIMIT ?2",
            Self::SUMMARY_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![before.unwrap_or(i64::MAX), limit], Self::row_to_summary)?;
        rows.collect()
    }

    pub fn get_activity(&self, id: i64) -> rusqlite::Result<Option<ActivityDetail>> {
        let sql = format!("SELECT {} FROM activities a WHERE a.id = ?1", Self::SUMMARY_COLS);
        let Some(activity) = self.conn.query_row(&sql, params![id], Self::row_to_summary).optional()? else {
            return Ok(None);
        };
        let mut stmt = self.conn.prepare(
            "SELECT id, captured_at, text, COALESCE(raw_text, text), image FROM snapshots WHERE activity_id = ?1 ORDER BY captured_at ASC",
        )?;
        let snapshots = stmt
            .query_map(params![id], |r| Ok(Snapshot { id: r.get(0)?, captured_at: r.get(1)?, text: r.get(2)?, raw: r.get(3)?, image: r.get(4)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(ActivityDetail { activity, snapshots }))
    }

    /// Keyword search over window titles, URLs, app names and captured text.
    /// Every word must match (prefix match); falls back to any-word when
    /// that finds nothing. Results are grouped per activity.
    pub fn search(&self, query: &str, limit: u32) -> rusqlite::Result<Vec<SearchHit>> {
        let words: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"*", w.to_lowercase()))
            .collect();
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let hits = self.search_fts(&words.join(" "), limit)?;
        if !hits.is_empty() || words.len() == 1 {
            return Ok(hits);
        }
        self.search_fts(&words.join(" OR "), limit)
    }

    fn search_fts(&self, fts_query: &str, limit: u32) -> rusqlite::Result<Vec<SearchHit>> {
        // (activity_id -> (best bm25, snippet)). bm25 is lower-is-better.
        let mut best: std::collections::HashMap<i64, (f64, String)> = std::collections::HashMap::new();

        let mut consider = |activity_id: i64, score: f64, snippet: String| {
            let entry = best.entry(activity_id).or_insert((f64::MAX, String::new()));
            if score < entry.0 {
                *entry = (score, snippet);
            }
        };

        let snippet_args = format!("char({}), char({}), '…', 18", HL_START as u32, HL_END as u32);

        let sql = format!(
            "SELECT s.activity_id, bm25(snapshots_fts), snippet(snapshots_fts, 0, {snippet_args})
             FROM snapshots_fts JOIN snapshots s ON s.id = snapshots_fts.rowid
             WHERE snapshots_fts MATCH ?1 ORDER BY bm25(snapshots_fts) LIMIT 500"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        for row in stmt.query_map(params![fts_query], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))? {
            let (id, score, snip) = row?;
            consider(id, score, snip);
        }

        // Title/URL matches are strong signals: weight them above body text.
        let sql = format!(
            "SELECT rowid, bm25(activities_fts) * 2.0, snippet(activities_fts, -1, {snippet_args})
             FROM activities_fts WHERE activities_fts MATCH ?1 ORDER BY bm25(activities_fts) LIMIT 500"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        for row in stmt.query_map(params![fts_query], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))? {
            let (id, score, snip) = row?;
            consider(id, score, snip);
        }

        let mut ranked: Vec<(i64, f64, String)> = best.into_iter().map(|(id, (s, snip))| (id, s, snip)).collect();
        ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(limit as usize);

        let sql = format!("SELECT {} FROM activities a WHERE a.id = ?1", Self::SUMMARY_COLS);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut hits = Vec::with_capacity(ranked.len());
        for (id, _, snippet) in ranked {
            if let Some(activity) = stmt.query_row(params![id], Self::row_to_summary).optional()? {
                hits.push(SearchHit { activity, snippet });
            }
        }
        Ok(hits)
    }

    // ── Memories ──────────────────────────────────────────────────────────

    /// Mark an activity as asked for, so the engine writes its memory next.
    pub fn mark_activity_urgent(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE activities SET memory_status = 'urgent' WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Oldest closed activity still waiting for the memory engine. Textless
    /// closed activities are marked skipped on the way.
    pub fn next_pending_activity(&self, ended_before: i64) -> rusqlite::Result<Option<PendingActivity>> {
        self.conn.execute(
            "UPDATE activities SET memory_status = 'skipped'
             WHERE memory_status = 'pending' AND snapshot_count = 0 AND ended_at < ?1",
            params![ended_before],
        )?;
        // A capture the person asked for is marked 'urgent' and jumps the
        // settling queue: they pressed the button, they expect it written.
        self.conn
            .query_row(
                "SELECT id, app_name, window_title, url, started_at, ended_at FROM activities
                 WHERE snapshot_count > 0
                   AND (memory_status = 'urgent' OR (memory_status = 'pending' AND ended_at < ?1))
                 ORDER BY (memory_status = 'urgent') DESC, started_at ASC LIMIT 1",
                params![ended_before],
                |r| {
                    Ok(PendingActivity {
                        id: r.get(0)?,
                        app_name: r.get(1)?,
                        window_title: r.get(2)?,
                        url: r.get(3)?,
                        started_at: r.get(4)?,
                        ended_at: r.get(5)?,
                    })
                },
            )
            .optional()
    }

    /// The activity's clean snapshots merged into one text, distinct lines
    /// in first-seen order, capped for the model's context.
    pub fn activity_text(&self, id: i64, max_chars: usize) -> rusqlite::Result<String> {
        let mut stmt = self.conn.prepare("SELECT text FROM snapshots WHERE activity_id = ?1 ORDER BY captured_at, id")?;
        let mut seen = std::collections::HashSet::new();
        let mut out = String::new();
        for text in stmt.query_map(params![id], |r| r.get::<_, String>(0))? {
            for line in text?.lines() {
                if seen.insert(line.to_string()) {
                    if out.len() + line.len() + 1 > max_chars {
                        return Ok(out);
                    }
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(line);
                }
            }
        }
        Ok(out)
    }

    pub fn set_memory_status(&self, activity_id: i64, status: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE activities SET memory_status = ?2 WHERE id = ?1", params![activity_id, status])?;
        Ok(())
    }

    pub fn insert_memory(&self, activity_id: i64, m: &NewMemory, at: i64) -> rusqlite::Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        // A memory the user edited or pinned is theirs: reprocessing must
        // not overwrite it.
        let protected: Option<i64> = tx
            .query_row("SELECT id FROM memories WHERE activity_id = ?1 AND (pinned = 1 OR edited_at IS NOT NULL)", params![activity_id], |r| r.get(0))
            .optional()?;
        if let Some(id) = protected {
            tx.execute("UPDATE activities SET memory_status = 'done' WHERE id = ?1", params![activity_id])?;
            tx.commit()?;
            return Ok(id);
        }
        // Facts the user added or corrected outlive reprocessing.
        let user_facts: Vec<(String, String, String, i64, String, i64)> = tx
            .prepare("SELECT f.subject, f.attribute, f.value, f.as_of, f.status, f.created_at FROM facts f JOIN memories mm ON mm.id = f.memory_id WHERE mm.activity_id = ?1 AND f.origin = 'user'")?
            .query_map(params![activity_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
            .collect::<rusqlite::Result<_>>()?;
        // Reprocessing replaces the old memory but keeps the user's verdict.
        let feedback: Option<String> = tx
            .query_row("SELECT feedback FROM memories WHERE activity_id = ?1", params![activity_id], |r| r.get(0))
            .optional()?
            .flatten();
        // Tasks (and the user's decisions on them) outlive reprocessing.
        let old_tasks: Vec<(String, String, String, i64, Option<i64>)> = tx
            .prepare("SELECT t.text, t.normalized, t.status, t.created_at, t.closed_at FROM tasks t JOIN memories mm ON mm.id = t.memory_id WHERE mm.activity_id = ?1")?
            .query_map(params![activity_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<rusqlite::Result<_>>()?;
        tx.execute("DELETE FROM memories WHERE activity_id = ?1", params![activity_id])?;
        let group_key: String = tx.query_row(
            "SELECT app_name, window_title, url FROM activities WHERE id = ?1",
            params![activity_id],
            |r| Ok(clean::group_key(&r.get::<_, String>(0)?, &r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?.as_deref())),
        )?;
        tx.execute(
            "INSERT INTO memories(activity_id, created_at, kind, title, summary, people, organizations, dates, numbers,
                                  keep, confidence, dropped, model, feedback, group_key, projects, decisions)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                activity_id, at, m.kind, m.title, m.summary,
                serde_json::to_string(&m.people).unwrap_or_default(),
                serde_json::to_string(&m.organizations).unwrap_or_default(),
                serde_json::to_string(&m.dates).unwrap_or_default(),
                serde_json::to_string(&m.numbers).unwrap_or_default(),
                m.keep, m.confidence, m.dropped, m.model, feedback, group_key,
                serde_json::to_string(&m.projects).unwrap_or_default(),
                serde_json::to_string(&m.decisions).unwrap_or_default()
            ],
        )?;
        let id = tx.last_insert_rowid();
        let seen_at: i64 = tx.query_row("SELECT started_at FROM activities WHERE id = ?1", params![activity_id], |r| r.get(0))?;
        if m.keep {
            for f in m.facts.iter().take(8) {
                let key = fact_key(&f.subject, &f.attribute);
                if key.len() < 4 || f.value.trim().is_empty() {
                    continue;
                }
                // A user's fact with the same key wins over the model's.
                if user_facts.iter().any(|u| fact_key(&u.0, &u.1) == key) {
                    continue;
                }
                tx.execute(
                    "INSERT INTO facts(memory_id, subject, attribute, value, key, as_of, origin, status, created_at, owner, stance) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'model', 'active', ?7, ?8, ?9)",
                    params![id, f.subject.trim(), f.attribute.trim(), f.value.trim(), key, seen_at, at, f.owner, f.stance],
                )?;
            }
        }
        for (subject, attribute, value, as_of, status, created_at) in user_facts {
            tx.execute(
                "INSERT INTO facts(memory_id, subject, attribute, value, key, as_of, origin, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'user', ?7, ?8)",
                params![id, subject, attribute, value, fact_key(&subject, &attribute), as_of, status, created_at],
            )?;
        }
        for (kind, names) in [("person", &m.people), ("org", &m.organizations), ("project", &m.projects)] {
            for name in names {
                let norm = normalize_entity(name);
                if norm.chars().count() < 2 {
                    continue;
                }
                tx.execute(
                    "INSERT INTO entities(kind, name, normalized, first_seen, last_seen, mentions) VALUES (?1, ?2, ?3, ?4, ?4, 1)
                     ON CONFLICT(kind, normalized) DO UPDATE SET mentions = mentions + 1,
                        last_seen = MAX(last_seen, excluded.last_seen), first_seen = MIN(first_seen, excluded.first_seen)",
                    params![kind, name.trim(), norm, seen_at],
                )?;
                let eid: i64 = tx.query_row("SELECT id FROM entities WHERE kind = ?1 AND normalized = ?2", params![kind, norm], |r| r.get(0))?;
                tx.execute("INSERT OR IGNORE INTO memory_entities(memory_id, entity_id) VALUES (?1, ?2)", params![id, eid])?;
            }
        }
        // Only what was judged worth keeping can create work for the user.
        for task in m.tasks.iter().filter(|_| m.keep) {
            let norm = normalize_entity(task);
            if norm.chars().count() < 6 {
                continue;
            }
            // The same commitment seen again (another session of the same
            // page) stays one task, and a task the user closed stays closed.
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks t JOIN memories mm ON mm.id = t.memory_id WHERE t.normalized = ?1 AND mm.group_key = ?2)",
                params![norm, group_key],
                |r| r.get(0),
            )?;
            // A rephrasing of a commitment already open (the same document
            // drafted over several sittings) is not a second commitment.
            let rephrased = if exists {
                false
            } else {
                let mut st = tx.prepare("SELECT text FROM tasks WHERE status = 'open' AND created_at > ?1")?;
                let recent: Vec<String> = st.query_map(params![at - 60 * 86_400_000], |r| r.get(0))?.filter_map(Result::ok).collect();
                recent.iter().any(|r| crate::engine::same_task(r, task))
            };
            if !exists && !rephrased {
                tx.execute(
                    "INSERT INTO tasks(memory_id, text, normalized, status, created_at) VALUES (?1, ?2, ?3, 'open', ?4)",
                    params![id, task.trim(), norm, at],
                )?;
            }
        }
        for (text, norm, status, created_at, closed_at) in old_tasks {
            tx.execute(
                "INSERT INTO tasks(memory_id, text, normalized, status, created_at, closed_at)
                 SELECT ?1, ?2, ?3, ?4, ?5, ?6 WHERE NOT EXISTS (SELECT 1 FROM tasks WHERE memory_id = ?1 AND normalized = ?3)",
                params![id, text, norm, status, created_at, closed_at],
            )?;
            // A new extraction of a task the user already closed keeps its status.
            tx.execute("UPDATE tasks SET status = ?2, closed_at = ?3 WHERE memory_id = ?1 AND normalized = ?4 AND ?2 != 'open'", params![id, status, closed_at, norm])?;
        }
        tx.execute("UPDATE activities SET memory_status = 'done' WHERE id = ?1", params![activity_id])?;
        tx.commit()?;
        Ok(id)
    }

    // ── Entities, graph, tasks ────────────────────────────────────────────

    pub fn list_entities(&self, kind: Option<&str>, query: Option<&str>, limit: u32) -> rusqlite::Result<Vec<Entity>> {
        let like = format!("%{}%", query.unwrap_or("").to_lowercase());
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, name, mentions, first_seen, last_seen FROM entities
             WHERE (?1 IS NULL OR kind = ?1) AND normalized LIKE ?2 ORDER BY mentions DESC, last_seen DESC LIMIT ?3",
        )?;
        let out: Vec<Entity> = stmt
            .query_map(params![kind, like, limit], |r| {
                Ok(Entity { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)?, mentions: r.get(3)?, first_seen: r.get(4)?, last_seen: r.get(5)? })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn entity_memories(&self, entity_id: i64, limit: u32) -> rusqlite::Result<Vec<MemoryCard>> {
        let sql = format!(
            "SELECT {} FROM memory_entities me JOIN memories m ON m.id = me.memory_id JOIN activities a ON a.id = m.activity_id
             WHERE me.entity_id = ?1 ORDER BY a.started_at DESC LIMIT ?2",
            Self::MEMORY_COLS
        );
        let cards: Vec<MemoryCard> = self.conn.prepare(&sql)?.query_map(params![entity_id, limit * 4], Self::row_to_memory)?.collect::<rusqlite::Result<_>>()?;
        Ok(Self::group_cards(cards).into_iter().take(limit as usize).collect())
    }

    /// The most-mentioned entities and how often they appear in the same
    /// memory. `min_mentions` keeps one-off names off the board.
    pub fn graph(&self, max_nodes: u32, min_mentions: i64) -> rusqlite::Result<Graph> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, name, mentions, first_seen, last_seen FROM entities WHERE mentions >= ?1 ORDER BY mentions DESC LIMIT ?2",
        )?;
        let nodes: Vec<Entity> = stmt
            .query_map(params![min_mentions, max_nodes], |r| {
                Ok(Entity { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)?, mentions: r.get(3)?, first_seen: r.get(4)?, last_seen: r.get(5)? })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let ids: std::collections::HashSet<i64> = nodes.iter().map(|n| n.id).collect();
        let mut stmt = self.conn.prepare(
            "SELECT a.entity_id, b.entity_id, COUNT(*) FROM memory_entities a JOIN memory_entities b
             ON a.memory_id = b.memory_id AND a.entity_id < b.entity_id GROUP BY a.entity_id, b.entity_id",
        )?;
        let edges = stmt
            .query_map([], |r| Ok(GraphEdge { source: r.get(0)?, target: r.get(1)?, weight: r.get(2)? }))?
            .filter_map(Result::ok)
            .filter(|e| ids.contains(&e.source) && ids.contains(&e.target))
            .collect();
        Ok(Graph { nodes, edges })
    }

    pub fn list_tasks(&self, status: &str, limit: u32) -> rusqlite::Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.memory_id, t.text, t.status, t.created_at, m.title, a.app_name, a.started_at, a.id
             FROM tasks t JOIN memories m ON m.id = t.memory_id JOIN activities a ON a.id = m.activity_id
             WHERE t.status = ?1 AND m.feedback IS NOT 'ignore' ORDER BY a.started_at DESC LIMIT ?2",
        )?;
        let out: Vec<Task> = stmt
            .query_map(params![status, limit], |r| {
                Ok(Task {
                    id: r.get(0)?, memory_id: r.get(1)?, text: r.get(2)?, status: r.get(3)?, created_at: r.get(4)?,
                    title: r.get(5)?, app_name: r.get(6)?, started_at: r.get(7)?, activity_id: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// Open tasks whose text contains every query word (3+ letters).
    /// Open commitments from memories that mention this person, org or project.
    pub fn tasks_for_entity(&self, entity_id: i64, limit: u32) -> rusqlite::Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.memory_id, t.text, t.status, t.created_at, m.title, a.app_name, a.started_at, a.id
             FROM tasks t JOIN memories m ON m.id = t.memory_id JOIN activities a ON a.id = m.activity_id
             JOIN memory_entities me ON me.memory_id = m.id
             WHERE me.entity_id = ?1 AND t.status = 'open' AND m.feedback IS NOT 'ignore' ORDER BY a.started_at DESC LIMIT ?2",
        )?;
        let out: Vec<Task> = stmt
            .query_map(params![entity_id, limit], |r| Ok(Task { id: r.get(0)?, memory_id: r.get(1)?, text: r.get(2)?, status: r.get(3)?, created_at: r.get(4)?, title: r.get(5)?, app_name: r.get(6)?, started_at: r.get(7)?, activity_id: r.get(8)? }))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn search_tasks(&self, query: &str, limit: u32) -> rusqlite::Result<Vec<Task>> {
        let words: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 3)
            .map(|w| w.to_lowercase())
            .collect();
        if words.is_empty() {
            return Ok(vec![]);
        }
        let conds = (0..words.len()).map(|i| format!("instr(t.normalized, ?{}) > 0", i + 1)).collect::<Vec<_>>().join(" AND ");
        let sql = format!(
            "SELECT t.id, t.memory_id, t.text, t.status, t.created_at, m.title, a.app_name, a.started_at, a.id
             FROM tasks t JOIN memories m ON m.id = t.memory_id JOIN activities a ON a.id = m.activity_id
             WHERE t.status = 'open' AND m.feedback IS NOT 'ignore' AND {conds} ORDER BY a.started_at DESC LIMIT {limit}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let out: Vec<Task> = stmt
            .query_map(rusqlite::params_from_iter(words.iter()), |r| {
                Ok(Task {
                    id: r.get(0)?, memory_id: r.get(1)?, text: r.get(2)?, status: r.get(3)?, created_at: r.get(4)?,
                    title: r.get(5)?, app_name: r.get(6)?, started_at: r.get(7)?, activity_id: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// Entities whose name contains a query word of 3+ letters, most
    /// mentioned first. Used to answer "who is X" from everything about X.
    pub fn entities_named_in(&self, query: &str, limit: u32) -> rusqlite::Result<Vec<Entity>> {
        let mut out: Vec<Entity> = Vec::new();
        for w in query.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3) {
            for e in self.list_entities(None, Some(w), limit)? {
                if !out.iter().any(|x| x.id == e.id) {
                    out.push(e);
                }
            }
            if let Some(e) = self.entity_by_alias(w)? {
                if !out.iter().any(|x| x.id == e.id) {
                    out.push(e);
                }
            }
        }
        out.sort_by(|a, b| b.mentions.cmp(&a.mentions));
        out.truncate(limit as usize);
        Ok(out)
    }

    /// Everything known about an entity, as text for the model: how often,
    /// when, with whom, and the latest memories.
    pub fn entity_brief(&self, id: i64, max_memories: u32) -> rusqlite::Result<String> {
        let e = self
            .conn
            .query_row("SELECT kind, name, mentions, first_seen, last_seen FROM entities WHERE id = ?1", params![id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
            })?;
        let mut stmt = self.conn.prepare(
            "SELECT e.name, COUNT(*) c FROM memory_entities a JOIN memory_entities b ON a.memory_id = b.memory_id AND b.entity_id != a.entity_id
             JOIN entities e ON e.id = b.entity_id WHERE a.entity_id = ?1 GROUP BY b.entity_id ORDER BY c DESC LIMIT 6",
        )?;
        let with: Vec<String> = stmt.query_map(params![id], |r| Ok(format!("{} ({}×)", r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let cards = self.entity_memories(id, max_memories)?;
        let kind = match e.0.as_str() { "person" => "Person", "org" => "Organisation", _ => "Project" };
        let mut out = format!("{kind}: {} · mentioned {} times\nAppears with: {}\nRecent:\n", e.1, e.2, if with.is_empty() { "—".into() } else { with.join(", ") });
        for c in cards {
            out.push_str(&format!("- {} — {}\n", c.title, c.summary));
        }
        Ok(out)
    }

    pub fn set_task_status(&self, id: i64, status: &str, at: i64) -> rusqlite::Result<()> {
        let closed = (status != "open").then_some(at);
        self.conn.execute("UPDATE tasks SET status = ?2, closed_at = ?3 WHERE id = ?1", params![id, status, closed])?;
        Ok(())
    }

    const MEMORY_COLS: &'static str = "m.id, m.activity_id, m.created_at, m.kind, m.title, m.summary, m.people, m.organizations,
        m.dates, m.numbers, m.keep, m.confidence, m.dropped, m.model, m.feedback,
        a.app_name, a.window_title, a.url, a.started_at, a.ended_at, m.group_key, m.projects, m.decisions, m.pinned, m.edited_at";

    fn row_to_memory(r: &rusqlite::Row) -> rusqlite::Result<MemoryCard> {
        let list = |i: usize| -> rusqlite::Result<Vec<String>> {
            Ok(serde_json::from_str(&r.get::<_, String>(i)?).unwrap_or_default())
        };
        Ok(MemoryCard {
            id: r.get(0)?,
            activity_id: r.get(1)?,
            created_at: r.get(2)?,
            kind: r.get(3)?,
            title: r.get(4)?,
            summary: r.get(5)?,
            people: list(6)?,
            organizations: list(7)?,
            dates: list(8)?,
            numbers: list(9)?,
            projects: list(21)?,
            decisions: list(22)?,
            keep: r.get(10)?,
            confidence: r.get(11)?,
            dropped: r.get(12)?,
            model: r.get(13)?,
            feedback: r.get(14)?,
            app_name: r.get(15)?,
            window_title: r.get(16)?,
            url: r.get(17)?,
            started_at: r.get(18)?,
            ended_at: r.get(19)?,
            group_key: r.get(20)?,
            sessions: 1,
            total_ms: 0,
            ids: vec![r.get(0)?],
            pinned: r.get::<_, i64>(23)? != 0,
            edited_at: r.get(24)?,
            facts: Vec::new(),
        })
    }

    /// Merge sessions of the same thing (same group key, within 4 hours of
    /// each other) into one card. Input is newest first; output too.
    pub fn group_cards(cards: Vec<MemoryCard>) -> Vec<MemoryCard> {
        const GAP_MS: i64 = 4 * 3_600_000;
        let mut out: Vec<MemoryCard> = Vec::new();
        let mut longest: Vec<i64> = Vec::new();
        for c in cards {
            let found = out.iter().position(|g| g.group_key == c.group_key && g.started_at - c.ended_at <= GAP_MS && g.started_at >= c.started_at);
            match found.map(|i| (&mut out[i], &mut longest[i])) {
                Some((g, longest_ms)) => {
                    g.sessions += 1;
                    g.total_ms += c.ended_at - c.started_at;
                    g.started_at = g.started_at.min(c.started_at);
                    g.ids.push(c.id);
                    for (into, from) in [(&mut g.people, &c.people), (&mut g.organizations, &c.organizations), (&mut g.dates, &c.dates), (&mut g.numbers, &c.numbers), (&mut g.projects, &c.projects), (&mut g.decisions, &c.decisions)] {
                        for x in from {
                            if !into.iter().any(|y| y.eq_ignore_ascii_case(x)) {
                                into.push(x.clone());
                            }
                        }
                    }
                    // The longest session names the card.
                    if c.ended_at - c.started_at > *longest_ms {
                        *longest_ms = c.ended_at - c.started_at;
                        g.title = c.title.clone();
                        g.summary = c.summary.clone();
                    }
                    g.keep = g.keep || c.keep;
                    g.dropped += c.dropped;
                    if g.feedback.is_none() {
                        g.feedback = c.feedback.clone();
                    }
                }
                None => {
                    let mut g = c;
                    g.total_ms = g.ended_at - g.started_at;
                    longest.push(g.total_ms);
                    out.push(g);
                }
            }
        }
        out
    }

    /// Memory cards, newest first. `kept_only` hides what the model judged
    /// not worth keeping unless the user overruled it. `query` searches
    /// titles, summaries and extracted entities.
    pub fn list_memories(&self, query: Option<&str>, kept_only: bool, limit: u32) -> rusqlite::Result<Vec<MemoryCard>> {
        let visible = if kept_only {
            "((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')"
        } else {
            "1 = 1"
        };
        let words: Vec<String> = query
            .unwrap_or("")
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"*", w.to_lowercase()))
            .collect();
        if !words.is_empty() {
            return self.search_memories(query.unwrap_or(""), None, kept_only, limit);
        }
        {
            let sql = format!(
                "SELECT {} FROM memories m JOIN activities a ON a.id = m.activity_id WHERE {visible}
                 ORDER BY a.started_at DESC LIMIT ?1",
                Self::MEMORY_COLS
            );
            let cards: Vec<MemoryCard> = self.conn.prepare(&sql)?.query_map(params![limit * 4], Self::row_to_memory)?.collect::<rusqlite::Result<_>>()?;
            return Ok(Self::group_cards(cards).into_iter().take(limit as usize).collect());
        }
    }

    // ── Vectors ───────────────────────────────────────────────────────────

    /// Text that represents a memory for embedding: what the model made of
    /// it plus the start of the source.
    pub fn embedding_text(&self, memory_id: i64) -> rusqlite::Result<String> {
        self.conn.query_row(
            "SELECT m.title, m.summary, m.people, m.organizations, m.dates, m.numbers, m.activity_id FROM memories m WHERE m.id = ?1",
            params![memory_id],
            |r| {
                let activity_id: i64 = r.get(6)?;
                let excerpt = self.activity_text(activity_id, 800)?;
                Ok(format!(
                    "search_document: {}\n{}\n{} {} {} {}\n{}",
                    r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?,
                    excerpt
                ))
            },
        )
    }

    pub fn memories_without_vectors(&self, limit: u32) -> rusqlite::Result<Vec<i64>> {
        self.conn
            .prepare("SELECT id FROM memories WHERE id NOT IN (SELECT memory_id FROM memory_vectors) ORDER BY id DESC LIMIT ?1")?
            .query_map(params![limit], |r| r.get(0))?
            .collect()
    }

    pub fn store_vector(&self, memory_id: i64, model: &str, vec: &[f32]) -> rusqlite::Result<()> {
        let bytes: Vec<u8> = vec.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.conn.execute(
            "INSERT INTO memory_vectors(memory_id, model, vec) VALUES (?1, ?2, ?3)
             ON CONFLICT(memory_id) DO UPDATE SET model = excluded.model, vec = excluded.vec",
            params![memory_id, model, bytes],
        )?;
        if let Some(idx) = self.vindex.borrow_mut().as_mut() {
            if idx.ids.contains(&memory_id) {
                *self.vindex.borrow_mut() = None; // replaced: rebuild lazily
            } else {
                idx.push(memory_id, vec);
            }
        }
        Ok(())
    }

    fn memory_index(&self) -> rusqlite::Result<()> {
        if self.vindex.borrow().is_some() {
            return Ok(());
        }
        let mut stmt = self.conn.prepare("SELECT memory_id, vec FROM memory_vectors")?;
        let mut idx: Option<QuantIndex> = None;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))? {
            let (id, bytes) = row?;
            let v = bytes_to_f32(&bytes);
            let ix = idx.get_or_insert_with(|| QuantIndex::new(v.len()));
            ix.push(id, &v);
        }
        *self.vindex.borrow_mut() = Some(idx.unwrap_or_else(|| QuantIndex::new(0)));
        Ok(())
    }

    /// Forget the in-memory indexes (after deletes); they rebuild on demand.
    pub fn invalidate_indexes(&self) {
        *self.vindex.borrow_mut() = None;
        *self.findex.borrow_mut() = None;
    }

    /// Brute-force cosine over every vector. Fine for tens of thousands of
    /// memories; an index comes when that stops being true.
    pub fn vector_search(&self, query: &[f32], limit: usize) -> rusqlite::Result<Vec<(i64, f32)>> {
        self.vector_search_visible(query, false, limit)
    }

    fn vector_search_visible(&self, query: &[f32], kept_only: bool, limit: usize) -> rusqlite::Result<Vec<(i64, f32)>> {
        self.memory_index()?;
        let idx = self.vindex.borrow();
        let Some(idx) = idx.as_ref() else { return Ok(vec![]) };
        // Over-fetch, then keep only what is visible.
        let candidates = idx.search(query, limit * 4 + 16);
        // The index may lag a delete: every hit is checked against the table.
        let sql = if kept_only {
            format!("SELECT 1 FROM memories m JOIN memory_vectors v ON v.memory_id = m.id WHERE m.id = ?1 AND {}", Self::VISIBLE)
        } else {
            "SELECT 1 FROM memory_vectors v WHERE v.memory_id = ?1".to_string()
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let mut out = Vec::with_capacity(limit);
        for (id, score) in candidates {
            if stmt.query_row(params![id], |_| Ok(())).optional()?.is_some() {
                out.push((id, score));
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    const VISIBLE: &'static str = "((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')";

    /// Keyword candidates. Visibility is applied inside the query so that
    /// short "not worth keeping" memories (file names, menus) cannot crowd
    /// kept ones out of the pool.
    fn fts_memory_ids(&self, query: &str, kept_only: bool, limit: usize) -> rusqlite::Result<Vec<i64>> {
        let words: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"*", w.to_lowercase()))
            .collect();
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let visible = if kept_only { Self::VISIBLE } else { "1 = 1" };
        let sql = format!(
            "SELECT f.rowid FROM memories_fts f JOIN memories m ON m.id = f.rowid WHERE memories_fts MATCH ?1 AND {visible} ORDER BY bm25(memories_fts) LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let strict: Vec<i64> = stmt.query_map(params![words.join(" "), limit as i64], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        if !strict.is_empty() || words.len() == 1 {
            return Ok(strict);
        }
        let loose: Vec<i64> = stmt.query_map(params![words.join(" OR "), limit as i64], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(loose)
    }

    /// Hybrid search: keyword ranks and vector ranks fused with reciprocal
    /// rank fusion, then grouped into cards. With no vector, keyword only.
    pub fn search_memories(&self, query: &str, query_vec: Option<&[f32]>, kept_only: bool, limit: u32) -> rusqlite::Result<Vec<MemoryCard>> {
        self.search_memories_in(query, query_vec, kept_only, limit, None)
    }

    /// Same, restricted to memories whose activity started inside `window`
    /// (ms). With a window and few keyword hits, the window's own memories
    /// fill in, so "what did I do yesterday" works without matching words.
    pub fn search_memories_in(&self, query: &str, query_vec: Option<&[f32]>, kept_only: bool, limit: u32, window: Option<(i64, i64)>) -> rusqlite::Result<Vec<MemoryCard>> {
        let pool = (limit as usize) * if window.is_some() { 12 } else { 4 };
        let mut fused: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
        for (rank, id) in self.fts_memory_ids(query, kept_only, pool)?.into_iter().enumerate() {
            *fused.entry(id).or_default() += 1.0 / (60.0 + rank as f64);
        }
        if let Some(q) = query_vec {
            for (rank, (id, score)) in self.vector_search_visible(q, kept_only, pool)?.into_iter().enumerate() {
                if score < 0.35 {
                    break;
                }
                *fused.entry(id).or_default() += 1.0 / (60.0 + rank as f64);
            }
        }
        // A pinned memory that matched at all ranks as if it topped a list.
        let pinned = self.pinned_ids()?;
        for (id, score) in fused.iter_mut() {
            if pinned.contains(id) {
                *score += 1.0 / 60.0;
            }
        }
        let mut ranked: Vec<(i64, f64)> = fused.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let visible = if kept_only { "((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')" } else { "1 = 1" };
        let sql = format!("SELECT {} FROM memories m JOIN activities a ON a.id = m.activity_id WHERE m.id = ?1 AND {visible}", Self::MEMORY_COLS);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut cards = Vec::new();
        for (id, _) in ranked {
            if let Some(c) = stmt.query_row(params![id], Self::row_to_memory).optional()? {
                if let Some((since, until)) = window {
                    if c.started_at < since || c.started_at >= until {
                        continue;
                    }
                }
                cards.push(c);
            }
        }
        if let Some((since, until)) = window {
            if cards.len() < 3 {
                for c in self.memories_between(since, until, limit)? {
                    if !cards.iter().any(|x| x.id == c.id) {
                        cards.push(c);
                    }
                }
            }
        }
        // Group by key; a group's position is its best member's.
        let mut out: Vec<MemoryCard> = Vec::new();
        for c in cards {
            if let Some(g) = out.iter_mut().find(|g| g.group_key == c.group_key && (g.started_at - c.ended_at).abs() <= 4 * 3_600_000) {
                g.sessions += 1;
                g.total_ms += c.ended_at - c.started_at;
                g.ids.push(c.id);
                g.started_at = g.started_at.min(c.started_at);
            } else {
                let mut g = c;
                g.total_ms = g.ended_at - g.started_at;
                out.push(g);
            }
        }
        out.truncate(limit as usize);
        Ok(out)
    }

    // ── Recaps ────────────────────────────────────────────────────────────

    pub fn recap(&self, day: &str) -> rusqlite::Result<Option<(String, i64, String)>> {
        self.conn
            .query_row("SELECT text, generated_at, sources FROM recaps WHERE day = ?1", params![day], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()
    }

    pub fn save_recap(&self, day: &str, text: &str, sources_json: &str, at: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO recaps(day, text, generated_at, sources) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(day) DO UPDATE SET text = excluded.text, generated_at = excluded.generated_at, sources = excluded.sources",
            params![day, text, at, sources_json],
        )?;
        Ok(())
    }

    /// Kept memory groups in a time window, newest first.
    pub fn memories_between(&self, start: i64, end: i64, limit: u32) -> rusqlite::Result<Vec<MemoryCard>> {
        let sql = format!(
            "SELECT {} FROM memories m JOIN activities a ON a.id = m.activity_id
             WHERE a.started_at >= ?1 AND a.started_at < ?2 AND ((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')
             ORDER BY a.started_at DESC LIMIT ?3",
            Self::MEMORY_COLS
        );
        let cards: Vec<MemoryCard> = self.conn.prepare(&sql)?.query_map(params![start, end, limit * 4], Self::row_to_memory)?.collect::<rusqlite::Result<_>>()?;
        Ok(Self::group_cards(cards).into_iter().take(limit as usize).collect())
    }

    /// memory id → (kind, url), for every memory that has an open task.
    pub fn task_memory_kinds(&self) -> rusqlite::Result<std::collections::HashMap<i64, (String, Option<String>)>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT m.id, m.kind, a.url FROM memories m JOIN activities a ON a.id = m.activity_id JOIN tasks t ON t.memory_id = m.id WHERE t.status = 'open'")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().collect())
    }

    pub fn open_task_count(&self, start: i64, end: i64) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM tasks t JOIN memories m ON m.id = t.memory_id WHERE t.created_at >= ?1 AND t.created_at < ?2 AND t.status = 'open' AND m.feedback IS NOT 'ignore'",
            params![start, end],
            |r| r.get(0),
        )
    }

    pub fn tasks_between(&self, start: i64, end: i64) -> rusqlite::Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.memory_id, t.text, t.status, t.created_at, m.title, a.app_name, a.started_at, a.id
             FROM tasks t JOIN memories m ON m.id = t.memory_id JOIN activities a ON a.id = m.activity_id
             WHERE t.created_at >= ?1 AND t.created_at < ?2 AND t.status = 'open' AND m.feedback IS NOT 'ignore' ORDER BY t.created_at DESC LIMIT 30",
        )?;
        let out: Vec<Task> = stmt
            .query_map(params![start, end], |r| {
                Ok(Task {
                    id: r.get(0)?, memory_id: r.get(1)?, text: r.get(2)?, status: r.get(3)?, created_at: r.get(4)?,
                    title: r.get(5)?, app_name: r.get(6)?, started_at: r.get(7)?, activity_id: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// Decisions recorded in a window: (decision, memory title, activity id, when).
    pub fn decisions_between(&self, start: i64, end: i64) -> rusqlite::Result<Vec<(String, String, i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.decisions, m.title, a.id, a.started_at FROM memories m JOIN activities a ON a.id = m.activity_id
             WHERE a.started_at >= ?1 AND a.started_at < ?2 AND m.decisions != '[]' AND m.feedback IS NOT 'ignore' ORDER BY a.started_at DESC LIMIT 100",
        )?;
        let mut out = Vec::new();
        for row in stmt.query_map(params![start, end], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))? {
            let (json, title, aid, at) = row?;
            for d in serde_json::from_str::<Vec<String>>(&json).unwrap_or_default() {
                out.push((d, title.clone(), aid, at));
            }
        }
        Ok(out)
    }

    /// A note typed or dictated by the user becomes an activity with one
    /// snapshot, so it flows through the same memory engine.
    pub fn create_note(&self, text: &str, at: i64, kind: &str) -> rusqlite::Result<i64> {
        let title: String = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("Note").chars().take(80).collect();
        let a = self.start_activity(kind, None, title.trim(), None, at)?;
        self.extend_activity(a, at + 60_000)?;
        self.add_snapshot(a, text, at)?;
        Ok(a)
    }

    /// Minutes of captured activity per app in a window, largest first.
    pub fn time_by_app(&self, start: i64, end: i64) -> rusqlite::Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT app_name, SUM(ended_at - started_at) / 60000 AS mins FROM activities
             WHERE started_at >= ?1 AND started_at < ?2 GROUP BY app_name HAVING mins > 0 ORDER BY mins DESC LIMIT 8",
        )?;
        let out: Vec<(String, i64)> = stmt.query_map(params![start, end], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    // ── Board ─────────────────────────────────────────────────────────────

    pub fn board_positions(&self) -> rusqlite::Result<Vec<(String, f64, f64)>> {
        let mut stmt = self.conn.prepare("SELECT node_key, x, y FROM board_positions")?;
        let out: Vec<(String, f64, f64)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    // ── Conversations ──────────────────────────────────────────────────────

    pub fn create_conversation(&self, title: &str, at: i64) -> rusqlite::Result<i64> {
        let title: String = title.trim().chars().take(80).collect();
        self.conn.execute("INSERT INTO conversations(title, created_at, updated_at) VALUES (?1, ?2, ?2)", params![if title.is_empty() { "New chat" } else { &title }, at])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn append_message(&self, conversation_id: i64, role: &str, content: &str, sources_json: &str, at: i64) -> rusqlite::Result<i64> {
        self.conn.execute("INSERT INTO messages(conversation_id, role, content, sources, created_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![conversation_id, role, content, sources_json, at])?;
        self.conn.execute("UPDATE conversations SET updated_at = ?2 WHERE id = ?1", params![conversation_id, at])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_conversations(&self, limit: u32) -> rusqlite::Result<Vec<Conversation>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.title, c.created_at, c.updated_at, (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) FROM conversations c ORDER BY c.updated_at DESC LIMIT ?1",
        )?;
        let out: Vec<Conversation> = stmt.query_map(params![limit], |r| Ok(Conversation { id: r.get(0)?, title: r.get(1)?, created_at: r.get(2)?, updated_at: r.get(3)?, messages: r.get(4)? }))?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn conversation_messages(&self, id: i64) -> rusqlite::Result<Vec<Message>> {
        let mut stmt = self.conn.prepare("SELECT id, role, content, sources, created_at FROM messages WHERE conversation_id = ?1 ORDER BY id")?;
        let out: Vec<Message> = stmt.query_map(params![id], |r| Ok(Message { id: r.get(0)?, role: r.get(1)?, content: r.get(2)?, sources: r.get(3)?, created_at: r.get(4)? }))?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn rename_conversation(&self, id: i64, title: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE conversations SET title = ?2 WHERE id = ?1", params![id, title.trim().chars().take(80).collect::<String>()])?;
        Ok(())
    }

    pub fn delete_conversation(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn board_links(&self) -> rusqlite::Result<Vec<BoardLink>> {
        let mut stmt = self.conn.prepare("SELECT id, from_key, to_key, label, created_at FROM board_links ORDER BY id")?;
        let out: Vec<BoardLink> = stmt.query_map([], |r| Ok(BoardLink { id: r.get(0)?, from_key: r.get(1)?, to_key: r.get(2)?, label: r.get(3)?, created_at: r.get(4)? }))?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn add_board_link(&self, from_key: &str, to_key: &str, label: &str, at: i64) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO board_links(from_key, to_key, label, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(from_key, to_key) DO UPDATE SET label = excluded.label",
            params![from_key, to_key, label.trim(), at],
        )?;
        self.conn.query_row("SELECT id FROM board_links WHERE from_key = ?1 AND to_key = ?2", params![from_key, to_key], |r| r.get(0))
    }

    pub fn set_board_link_label(&self, id: i64, label: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE board_links SET label = ?2 WHERE id = ?1", params![id, label.trim()])?;
        Ok(())
    }

    pub fn remove_board_link(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM board_links WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn board_notes(&self) -> rusqlite::Result<Vec<BoardNote>> {
        let mut stmt = self.conn.prepare("SELECT node_key, text, created_at FROM board_notes ORDER BY created_at")?;
        let out: Vec<BoardNote> = stmt.query_map([], |r| Ok(BoardNote { key: r.get(0)?, text: r.get(1)?, created_at: r.get(2)? }))?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn set_board_note(&self, key: &str, text: &str, at: i64) -> rusqlite::Result<()> {
        self.conn.execute("INSERT INTO board_notes(node_key, text, created_at) VALUES (?1, ?2, ?3) ON CONFLICT(node_key) DO UPDATE SET text = excluded.text", params![key, text.trim(), at])?;
        Ok(())
    }

    /// Take a node off the board: its position, note and links.
    pub fn remove_board_node(&self, key: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM board_positions WHERE node_key = ?1", params![key])?;
        self.conn.execute("DELETE FROM board_notes WHERE node_key = ?1", params![key])?;
        self.conn.execute("DELETE FROM board_links WHERE from_key = ?1 OR to_key = ?1", params![key])?;
        Ok(())
    }

    pub fn set_board_position(&self, key: &str, pos: Option<(f64, f64)>) -> rusqlite::Result<()> {
        match pos {
            Some((x, y)) => {
                self.conn.execute("INSERT INTO board_positions(node_key, x, y) VALUES (?1, ?2, ?3) ON CONFLICT(node_key) DO UPDATE SET x = excluded.x, y = excluded.y", params![key, x, y])?;
            }
            None => {
                self.conn.execute("DELETE FROM board_positions WHERE node_key = ?1", params![key])?;
            }
        }
        Ok(())
    }

    /// Memory groups for the board in a time range: one node per group,
    /// with the entity ids it mentions.
    pub fn board_memories(&self, since: i64, limit: u32) -> rusqlite::Result<Vec<(MemoryCard, Vec<i64>)>> {
        let sql = format!(
            "SELECT {} FROM memories m JOIN activities a ON a.id = m.activity_id
             WHERE a.started_at >= ?1 AND ((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')
             ORDER BY a.started_at DESC LIMIT ?2",
            Self::MEMORY_COLS
        );
        let cards: Vec<MemoryCard> = self.conn.prepare(&sql)?.query_map(params![since, limit * 4], Self::row_to_memory)?.collect::<rusqlite::Result<_>>()?;
        let groups = Self::group_cards(cards).into_iter().take(limit as usize).collect::<Vec<_>>();
        let mut stmt = self.conn.prepare("SELECT DISTINCT entity_id FROM memory_entities WHERE memory_id = ?1")?;
        let mut out = Vec::with_capacity(groups.len());
        for g in groups {
            let mut ids: Vec<i64> = Vec::new();
            for mid in &g.ids {
                for e in stmt.query_map(params![mid], |r| r.get::<_, i64>(0))? {
                    let e = e?;
                    if !ids.contains(&e) {
                        ids.push(e);
                    }
                }
            }
            out.push((g, ids));
        }
        Ok(out)
    }

    // ── Meetings ──────────────────────────────────────────────────────────

    pub fn create_meeting(&self, title: &str, started_at: i64, audio_dir: &str) -> rusqlite::Result<(i64, i64)> {
        let activity = self.start_activity("Meeting", None, title, None, started_at)?;
        self.conn.execute(
            "INSERT INTO meetings(activity_id, title, started_at, status, audio_dir) VALUES (?1, ?2, ?3, 'recording', ?4)",
            params![activity, title, started_at, audio_dir],
        )?;
        Ok((self.conn.last_insert_rowid(), activity))
    }

    pub fn set_meeting_status(&self, id: i64, status: &str, detail: &str, ended_at: Option<i64>) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE meetings SET status = ?2, detail = ?3, ended_at = COALESCE(?4, ended_at) WHERE id = ?1",
            params![id, status, detail, ended_at],
        )?;
        Ok(())
    }

    pub fn list_meetings(&self, limit: u32) -> rusqlite::Result<Vec<Meeting>> {
        let mut stmt = self.conn.prepare(
            "SELECT mt.id, mt.activity_id, mt.title, mt.started_at, mt.ended_at, mt.status, mt.detail, m.title, m.summary, mt.notes, mt.attendees, mt.audio_dir, mt.speakers
             FROM meetings mt LEFT JOIN memories m ON m.activity_id = mt.activity_id ORDER BY mt.started_at DESC LIMIT ?1",
        )?;
        let out: Vec<Meeting> = stmt
            .query_map(params![limit], |r| {
                let attendees: String = r.get(10)?;
                let audio_dir: Option<String> = r.get(11)?;
                let speakers: String = r.get(12)?;
                Ok(Meeting {
                    id: r.get(0)?, activity_id: r.get(1)?, title: r.get(2)?, started_at: r.get(3)?, ended_at: r.get(4)?,
                    status: r.get(5)?, detail: r.get(6)?, memory_title: r.get(7)?, memory_summary: r.get(8)?,
                    notes: r.get(9)?,
                    attendees: serde_json::from_str(&attendees).unwrap_or_default(),
                    has_audio: audio_dir.map_or(false, |d| std::path::Path::new(&d).join("mic.wav").is_file()),
                    speakers: serde_json::from_str(&speakers).unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn set_meeting_notes(&self, id: i64, notes: &str, attendees: &[String]) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE meetings SET notes = ?2, attendees = ?3 WHERE id = ?1",
            params![id, notes, serde_json::to_string(attendees).unwrap_or_else(|_| "[]".into())],
        )?;
        Ok(())
    }

    // ── Signals and alignment ─────────────────────────────────────────────

    const SIGNAL_COLS: &'static str = "id, day, kind, ref_id, activity_id, title, reason, score, rank, state, features";

    fn signal_row(r: &rusqlite::Row) -> rusqlite::Result<Signal> {
        let features: String = r.get(10)?;
        Ok(Signal { id: r.get(0)?, day: r.get(1)?, kind: r.get(2)?, ref_id: r.get(3)?, activity_id: r.get(4)?, title: r.get(5)?, reason: r.get(6)?, score: r.get(7)?, rank: r.get(8)?, state: r.get(9)?, features: serde_json::from_str(&features).unwrap_or_default() })
    }

    /// Replace the day's open signals with a fresh ranking; what the user
    /// already pinned, finished or called noise keeps its state and place.
    pub fn replace_signals(&self, day: &str, fresh: &[(String, i64, i64, String, String, f64, std::collections::HashMap<String, f64>)], now: i64) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM signals WHERE day = ?1 AND state = 'open'", params![day])?;
        for (kind, ref_id, activity_id, title, reason, score, features) in fresh {
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM signals WHERE day = ?1 AND kind = ?2 AND ref_id = ?3 AND title = ?4)", params![day, kind, ref_id, title], |r| r.get(0))?;
            if exists {
                tx.execute("UPDATE signals SET reason = ?5, score = ?6, features = ?7 WHERE day = ?1 AND kind = ?2 AND ref_id = ?3 AND title = ?4", params![day, kind, ref_id, title, reason, score, serde_json::to_string(features).unwrap_or_default()])?;
            } else {
                tx.execute(
                    "INSERT INTO signals(day, kind, ref_id, activity_id, title, reason, score, rank, state, features, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 'open', ?8, ?9)",
                    params![day, kind, ref_id, activity_id, title, reason, score, serde_json::to_string(features).unwrap_or_default(), now],
                )?;
            }
        }
        // Rank: pinned first, then by score; noise and done sink.
        let mut stmt = tx.prepare("SELECT id FROM signals WHERE day = ?1 ORDER BY CASE state WHEN 'pinned' THEN 0 WHEN 'open' THEN 1 WHEN 'done' THEN 2 ELSE 3 END, score DESC")?;
        let ids: Vec<i64> = stmt.query_map(params![day], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        for (i, id) in ids.iter().enumerate() {
            tx.execute("UPDATE signals SET rank = ?2 WHERE id = ?1", params![id, i as i64 + 1])?;
        }
        tx.commit()
    }

    pub fn signals_for(&self, day: &str) -> rusqlite::Result<Vec<Signal>> {
        let sql = format!("SELECT {} FROM signals WHERE day = ?1 ORDER BY rank", Self::SIGNAL_COLS);
        let mut stmt = self.conn.prepare(&sql)?;
        let out = stmt.query_map(params![day], Self::signal_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(out)
    }

    pub fn signal_by_id(&self, id: i64) -> rusqlite::Result<Option<Signal>> {
        let sql = format!("SELECT {} FROM signals WHERE id = ?1", Self::SIGNAL_COLS);
        self.conn.query_row(&sql, params![id], Self::signal_row).optional()
    }

    pub fn set_signal_state(&self, id: i64, state: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE signals SET state = ?2 WHERE id = ?1", params![id, state])?;
        Ok(())
    }

    /// The user's own order for the day: ids first to last; the rest follow.
    pub fn rank_signals(&self, day: &str, ids: &[i64]) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute("UPDATE signals SET rank = ?2 WHERE id = ?1 AND day = ?3", params![id, i as i64 + 1, day])?;
        }
        let mut stmt = tx.prepare("SELECT id FROM signals WHERE day = ?1 ORDER BY rank")?;
        let rest: Vec<i64> = stmt.query_map(params![day], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        let mut n = ids.len() as i64;
        for id in rest {
            if !ids.contains(&id) {
                n += 1;
                tx.execute("UPDATE signals SET rank = ?2 WHERE id = ?1", params![id, n])?;
            }
        }
        tx.commit()
    }

    pub fn set_alignment_day(&self, day: &str, score: f64, memories: i64, near: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO alignment_days(day, score, memories, near) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(day) DO UPDATE SET score = excluded.score, memories = excluded.memories, near = excluded.near",
            params![day, score, memories, near],
        )?;
        Ok(())
    }

    /// (day, score, memories, near) for the last `days` days, oldest first.
    pub fn alignment_days(&self, days: i64) -> rusqlite::Result<Vec<(String, f64, i64, i64)>> {
        let mut stmt = self.conn.prepare("SELECT day, score, memories, near FROM alignment_days ORDER BY day DESC LIMIT ?1")?;
        let mut rows: Vec<(String, f64, i64, i64)> = stmt.query_map(params![days], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        rows.reverse();
        Ok(rows)
    }

    /// Entities of a kind that came up on at least `min_days` distinct days
    /// in the window: the threads running through the week.
    pub fn threads(&self, since: i64, min_days: i64, limit: u32) -> rusqlite::Result<Vec<(Entity, i64, i64)>> {
        let offset = crate::engine::day_bounds(since).0 % 86_400_000;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT e.id, e.kind, e.name, e.mentions, e.first_seen, e.last_seen, COUNT(DISTINCT (a.started_at - {offset}) / 86400000) d, COUNT(DISTINCT m.id) c
             FROM entities e JOIN memory_entities me ON me.entity_id = e.id JOIN memories m ON m.id = me.memory_id JOIN activities a ON a.id = m.activity_id
             WHERE a.started_at >= ?1 AND ((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')
             GROUP BY e.id HAVING d >= ?2 ORDER BY d DESC, c DESC LIMIT ?3"
        ))?;
        let rows = stmt
            .query_map(params![since, min_days, limit], |r| {
                Ok((Entity { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)?, mentions: r.get(3)?, first_seen: r.get(4)?, last_seen: r.get(5)? }, r.get::<_, i64>(6)?, r.get::<_, i64>(7)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// A memory older than `older_than` that mentions one of the entities.
    pub fn remember_when(&self, entity_ids: &[i64], older_than: i64) -> rusqlite::Result<Option<MemoryCard>> {
        if entity_ids.is_empty() {
            return Ok(None);
        }
        let list = entity_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT {} FROM memories m JOIN activities a ON a.id = m.activity_id JOIN memory_entities me ON me.memory_id = m.id
             WHERE me.entity_id IN ({list}) AND a.started_at < ?1 AND ((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep') ORDER BY RANDOM() LIMIT 1",
            Self::MEMORY_COLS
        );
        self.conn.query_row(&sql, params![older_than], Self::row_to_memory).optional()
    }

    /// Memory ids of the day (kept), with their vectors present or not.
    pub fn memory_ids_between(&self, start: i64, end: i64) -> rusqlite::Result<Vec<i64>> {
        let mut stmt = self.conn.prepare("SELECT m.id FROM memories m JOIN activities a ON a.id = m.activity_id WHERE a.started_at >= ?1 AND a.started_at < ?2 AND ((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')")?;
        let ids = stmt.query_map(params![start, end], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
        Ok(ids)
    }

    pub fn meeting_audio_dir(&self, id: i64) -> rusqlite::Result<Option<String>> {
        self.conn.query_row("SELECT audio_dir FROM meetings WHERE id = ?1", params![id], |r| r.get(0))
    }

    pub fn set_meeting_speakers(&self, id: i64, speakers: &[Speaker]) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE meetings SET speakers = ?2 WHERE id = ?1", params![id, serde_json::to_string(speakers).unwrap_or_else(|_| "[]".into())])?;
        Ok(())
    }

    /// Give a transcript speaker a name: "Speaker 2:" becomes "Sarah:" in every
    /// line of the meeting's snapshots (the FTS trigger follows the update).
    pub fn rename_speaker(&self, meeting_id: i64, label: &str, name: &str) -> rusqlite::Result<usize> {
        let activity = self.meeting_activity(meeting_id)?;
        let from = format!("] {label}: ");
        let to = format!("] {name}: ");
        let rows: Vec<(i64, String, Option<String>)> = self
            .conn
            .prepare("SELECT id, text, raw_text FROM snapshots WHERE activity_id = ?1 AND (instr(text, ?2) > 0 OR instr(COALESCE(raw_text, ''), ?2) > 0)")?
            .query_map(params![activity, from], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let n = rows.len();
        for (id, text, raw) in rows {
            self.conn.execute("UPDATE snapshots SET text = ?2, raw_text = ?3 WHERE id = ?1", params![id, text.replace(&from, &to), raw.map(|x| x.replace(&from, &to))])?;
        }
        let mut speakers: Vec<Speaker> = self
            .conn
            .query_row("SELECT speakers FROM meetings WHERE id = ?1", params![meeting_id], |r| r.get::<_, String>(0))
            .map(|j| serde_json::from_str(&j).unwrap_or_default())?;
        for sp in speakers.iter_mut() {
            if sp.label == label || sp.name == label {
                sp.name = name.to_string();
            }
        }
        self.set_meeting_speakers(meeting_id, &speakers)?;
        self.invalidate_indexes();
        Ok(n)
    }

    pub fn set_snapshot_image(&self, id: i64, path: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE snapshots SET image = ?2 WHERE id = ?1", params![id, path])?;
        Ok(())
    }

    /// Image files that belong to an activity (to remove with it), or to
    /// snapshots older than `before` (retention).
    pub fn images_of(&self, activity_id: Option<i64>, before: Option<i64>) -> rusqlite::Result<Vec<String>> {
        let mut out = Vec::new();
        if let Some(a) = activity_id {
            let mut stmt = self.conn.prepare("SELECT image FROM snapshots WHERE activity_id = ?1 AND image IS NOT NULL")?;
            out.extend(stmt.query_map(params![a], |r| r.get::<_, String>(0))?.filter_map(Result::ok));
        }
        if let Some(b) = before {
            let mut stmt = self.conn.prepare("SELECT image FROM snapshots WHERE captured_at < ?1 AND image IS NOT NULL")?;
            out.extend(stmt.query_map(params![b], |r| r.get::<_, String>(0))?.filter_map(Result::ok));
            self.conn.execute("UPDATE snapshots SET image = NULL WHERE captured_at < ?1 AND image IS NOT NULL", params![b])?;
        }
        Ok(out)
    }

    /// People you dealt with often who have not come up for a fortnight.
    pub fn going_quiet(&self, now: i64, limit: u32) -> rusqlite::Result<Vec<(i64, String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, last_seen FROM entities WHERE kind = 'person' AND mentions >= 4 AND last_seen < ?1 AND last_seen > ?2 ORDER BY mentions DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![now - 14 * 86_400_000, now - 60 * 86_400_000, limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Open commitments that have sat for more than a week.
    pub fn overdue_tasks(&self, now: i64, limit: u32) -> rusqlite::Result<Vec<Task>> {
        let all = self.list_tasks("open", 500)?;
        Ok(all.into_iter().filter(|t| t.created_at < now - 7 * 86_400_000).take(limit as usize).collect())
    }

    /// The newest documents in the index, one row each, for the Files page
    /// before a search is typed.
    pub fn list_files(&self, limit: u32) -> rusqlite::Result<Vec<FileHit>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.path, f.name, f.ext, f.mtime, f.size, COALESCE(substr(c.text, 1, 300), ''), f.url, COALESCE(c.id, 0)
             FROM files f LEFT JOIN file_chunks c ON c.file_id = f.id AND c.ord = 0 ORDER BY f.mtime DESC LIMIT ?1",
        )?;
        let out: Vec<FileHit> = stmt
            .query_map(params![limit], |r| Ok(FileHit { file_id: r.get(0)?, path: r.get(1)?, name: r.get(2)?, ext: r.get(3)?, mtime: r.get(4)?, size: r.get(5)?, snippet: r.get(6)?, url: r.get(7)?, chunk_id: r.get(8)? }))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn entity_memories_for_activity(&self, activity_id: i64) -> rusqlite::Result<Vec<MemoryCard>> {
        let sql = format!("SELECT {} FROM memories m JOIN activities a ON a.id = m.activity_id WHERE m.activity_id = ?1", Self::MEMORY_COLS);
        let out: Vec<MemoryCard> = self.conn.prepare(&sql)?.query_map(params![activity_id], Self::row_to_memory)?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn tasks_for_activity(&self, activity_id: i64) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.text FROM tasks t JOIN memories m ON m.id = t.memory_id WHERE m.activity_id = ?1 AND t.status != 'dismissed' ORDER BY t.id",
        )?;
        let out: Vec<String> = stmt.query_map(params![activity_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn meeting_activity(&self, id: i64) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT activity_id FROM meetings WHERE id = ?1", params![id], |r| r.get(0))
    }

    pub fn extend_activity_of_meeting(&self, id: i64, ended_at: i64) -> rusqlite::Result<()> {
        let activity = self.meeting_activity(id)?;
        self.extend_activity(activity, ended_at)
    }

    pub fn rename_meeting(&self, id: i64, title: &str) -> rusqlite::Result<()> {
        let activity: i64 = self.conn.query_row("SELECT activity_id FROM meetings WHERE id = ?1", params![id], |r| r.get(0))?;
        self.conn.execute("UPDATE meetings SET title = ?2 WHERE id = ?1", params![id, title])?;
        self.conn.execute("UPDATE activities SET window_title = ?2 WHERE id = ?1", params![activity, title])?;
        Ok(())
    }

    // ── Files ─────────────────────────────────────────────────────────────

    /// Is this file already indexed at this size and modification time?
    pub fn file_is_current(&self, path: &str, size: u64, mtime: i64) -> rusqlite::Result<bool> {
        self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM files WHERE path = ?1 AND size = ?2 AND mtime = ?3)",
            params![path, size as i64, mtime],
            |r| r.get(0),
        )
    }

    /// Store a file's chunks, replacing any previous version. Empty text is
    /// recorded so the file is not re-read every pass.
    pub fn upsert_file(&self, path: &str, size: u64, mtime: i64, chunks: &[String], at: i64) -> rusqlite::Result<i64> {
        let p = std::path::Path::new(path);
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        self.upsert_file_named(path, &name, &ext, size, mtime, chunks, at)
    }

    /// Same, for sources that are not files on disk (e.g. `notion://<id>`),
    /// where the display name and kind come from the source.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_file_named(&self, path: &str, name: &str, ext: &str, size: u64, mtime: i64, chunks: &[String], at: i64) -> rusqlite::Result<i64> {
        let joined = chunks.join("\n");
        let hash = text_hash(&joined);
        let tx = self.conn.unchecked_transaction()?;
        let existing: Option<(i64, String)> = tx
            .query_row("SELECT id, text_hash FROM files WHERE path = ?1", params![path], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let status = if chunks.is_empty() { "empty" } else { "ok" };
        let id = match existing {
            Some((id, old_hash)) => {
                tx.execute(
                    "UPDATE files SET name = ?2, ext = ?3, size = ?4, mtime = ?5, text_hash = ?6, indexed_at = ?7, chunk_count = ?8, status = ?9 WHERE id = ?1",
                    params![id, name, ext, size as i64, mtime, hash, at, chunks.len() as i64, status],
                )?;
                if old_hash == hash {
                    tx.commit()?;
                    return Ok(id);
                }
                tx.execute("DELETE FROM file_chunks WHERE file_id = ?1", params![id])?;
                id
            }
            None => {
                tx.execute(
                    "INSERT INTO files(path, name, ext, size, mtime, text_hash, indexed_at, chunk_count, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![path, name, ext, size as i64, mtime, hash, at, chunks.len() as i64, status],
                )?;
                tx.last_insert_rowid()
            }
        };
        for (i, c) in chunks.iter().enumerate() {
            tx.execute("INSERT INTO file_chunks(file_id, ord, text) VALUES (?1, ?2, ?3)", params![id, i as i64, c])?;
        }
        tx.commit()?;
        Ok(id)
    }

    /// Drop index entries for files that no longer exist under the folders.
    pub fn remove_missing_files(&self, present: &std::collections::HashSet<String>, folders: &[String]) -> rusqlite::Result<usize> {
        let paths: Vec<(i64, String)> = self.conn.prepare("SELECT id, path FROM files")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut n = 0;
        for (id, path) in paths {
            let in_scope = folders.iter().any(|f| path.starts_with(f));
            if in_scope && !present.contains(&path) {
                self.conn.execute("DELETE FROM files WHERE id = ?1", params![id])?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// Drop indexed entries under a scheme/prefix that are no longer present
    /// (e.g. Notion pages the integration can no longer see).
    pub fn remove_files_with_prefix(&self, prefix: &str, present: &std::collections::HashSet<String>) -> rusqlite::Result<usize> {
        let paths: Vec<(i64, String)> = self
            .conn
            .prepare("SELECT id, path FROM files WHERE path LIKE ?1")?
            .query_map(params![format!("{prefix}%")], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut n = 0;
        for (id, path) in paths {
            if !present.contains(&path) {
                self.conn.execute("DELETE FROM files WHERE id = ?1", params![id])?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// Text of an indexed document by file name (newest if several), for
    /// apps that show a document but expose none of its text (WPS, Preview,
    /// Word): what the user is reading comes from the index instead.
    pub fn file_text_by_name(&self, name: &str, max_chars: usize) -> rusqlite::Result<Option<(String, String)>> {
        let found: Option<(i64, String)> = self
            .conn
            .query_row("SELECT id, path FROM files WHERE name = ?1 COLLATE NOCASE AND chunk_count > 0 ORDER BY mtime DESC LIMIT 1", params![name], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let Some((id, path)) = found else { return Ok(None) };
        let mut stmt = self.conn.prepare("SELECT text FROM file_chunks WHERE file_id = ?1 ORDER BY ord")?;
        let mut out = String::new();
        for chunk in stmt.query_map(params![id], |r| r.get::<_, String>(0))? {
            let chunk = chunk?;
            if out.chars().count() + chunk.chars().count() > max_chars {
                out.push_str(&chunk.chars().take(max_chars.saturating_sub(out.chars().count())).collect::<String>());
                break;
            }
            out.push_str(&chunk);
            out.push('\n');
        }
        Ok(Some((path, out)))
    }

    pub fn set_file_url(&self, path: &str, url: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE files SET url = ?2 WHERE path = ?1", params![path, url])?;
        Ok(())
    }

    pub fn file_url(&self, path: &str) -> rusqlite::Result<Option<String>> {
        self.conn.query_row("SELECT url FROM files WHERE path = ?1", params![path], |r| r.get(0)).optional()
    }

    /// Whether this exact content was already indexed under this path.
    pub fn file_has_hash(&self, path: &str, hash: &str) -> rusqlite::Result<bool> {
        self.conn.query_row("SELECT EXISTS(SELECT 1 FROM files WHERE path = ?1 AND text_hash = ?2)", params![path, hash], |r| r.get(0))
    }

    pub fn connector_run(&self, id: &str) -> rusqlite::Result<(i64, i64, String)> {
        Ok(self
            .conn
            .query_row("SELECT last_run, items, error FROM connector_runs WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?
            .unwrap_or((0, 0, String::new())))
    }

    pub fn set_connector_run(&self, id: &str, last_run: i64, items: i64, error: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO connector_runs(id, last_run, items, error) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET last_run = excluded.last_run, items = excluded.items, error = excluded.error",
            params![id, last_run, items, error],
        )?;
        Ok(())
    }

    /// An activity that did not come from the screen: a connector item, a
    /// note, an import. Rabbit makes a memory of it like any other.
    /// A copied text joins the current "Clipboard" activity (one per
    /// `session_ms` window) or starts a new one.
    pub fn clipboard_note(&self, text: &str, at: i64, session_ms: i64) -> rusqlite::Result<i64> {
        let current: Option<i64> = self
            .conn
            .query_row("SELECT id FROM activities WHERE app_name = 'Clipboard' AND ended_at >= ?1 ORDER BY ended_at DESC LIMIT 1", params![at - session_ms], |r| r.get(0))
            .optional()?;
        let id = match current {
            Some(id) => {
                self.extend_activity(id, at)?;
                id
            }
            None => {
                let title: String = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("Copied text").chars().take(80).collect();
                let a = self.start_activity("Clipboard", None, title.trim(), None, at)?;
                self.extend_activity(a, at + 1_000)?;
                a
            }
        };
        self.add_snapshot(id, &format!("Copied:\n{text}"), at)?;
        Ok(id)
    }

    pub fn create_source_activity(&self, app: &str, title: &str, url: Option<&str>, text: &str, at: i64) -> rusqlite::Result<i64> {
        let a = self.start_activity(app, None, title, url, at)?;
        self.extend_activity(a, at + 60_000)?;
        self.add_snapshot(a, text, at)?;
        Ok(a)
    }

    pub fn count_files_with_prefix(&self, prefix: &str) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM files WHERE path LIKE ?1", params![format!("{prefix}%")], |r| r.get(0))
    }

    /// Cached compiled profile, if it is newer than the entity's last mention.
    pub fn entity_profile(&self, id: i64) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row("SELECT profile, profile_at, last_seen FROM entities WHERE id = ?1", params![id], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
            })
            .map(|(p, at, seen)| p.filter(|t| !t.is_empty() && at >= seen))
    }

    pub fn set_entity_profile(&self, id: i64, profile: &str, at: i64) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE entities SET profile = ?2, profile_at = ?3 WHERE id = ?1", params![id, profile, at])?;
        Ok(())
    }

    pub fn set_alias(&self, entity_id: i64, alias: &str) -> rusqlite::Result<()> {
        let norm = normalize_entity(alias);
        if norm.chars().count() < 2 {
            return Ok(());
        }
        self.conn.execute("INSERT OR REPLACE INTO entity_aliases(alias, entity_id, shown) VALUES (?1, ?2, ?3)", params![norm, entity_id, alias.trim()])?;
        Ok(())
    }

    /// Entity id of a person by exact full name, if known.
    pub fn person_id_by_name(&self, full_name: &str) -> rusqlite::Result<Option<i64>> {
        self.conn.query_row("SELECT id FROM entities WHERE kind = 'person' AND normalized = ?1", params![normalize_entity(full_name)], |r| r.get(0)).optional()
    }

    pub fn alias_exists(&self, alias: &str) -> rusqlite::Result<bool> {
        self.conn.query_row("SELECT EXISTS(SELECT 1 FROM entity_aliases WHERE alias = ?1)", params![normalize_entity(alias)], |r| r.get(0))
    }

    pub fn remove_alias(&self, alias: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM entity_aliases WHERE alias = ?1", params![normalize_entity(alias)])?;
        Ok(())
    }

    pub fn aliases_of(&self, entity_id: i64) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT shown FROM entity_aliases WHERE entity_id = ?1 ORDER BY shown")?;
        let out: Vec<String> = stmt.query_map(params![entity_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn entity_by_alias(&self, alias: &str) -> rusqlite::Result<Option<Entity>> {
        self.conn
            .query_row(
                "SELECT e.id, e.kind, e.name, e.mentions, e.first_seen, e.last_seen FROM entity_aliases a JOIN entities e ON e.id = a.entity_id WHERE a.alias = ?1",
                params![normalize_entity(alias)],
                |r| Ok(Entity { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)?, mentions: r.get(3)?, first_seen: r.get(4)?, last_seen: r.get(5)? }),
            )
            .optional()
    }

    /// "Sarah" → "Sarah Khan" when exactly one known person has that first
    /// name (or an alias says so). Returns the query with full names added,
    /// so retrieval finds the person the user means.
    pub fn alias_expand(&self, query: &str) -> rusqlite::Result<String> {
        let mut extra: Vec<String> = Vec::new();
        let lower = query.to_lowercase();
        for w in query.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3) {
            if let Some(e) = self.entity_by_alias(w)? {
                if !lower.contains(&e.name.to_lowercase()) && !extra.contains(&e.name) {
                    extra.push(e.name);
                }
                continue;
            }
            let wl = w.to_lowercase();
            let mut stmt = self.conn.prepare(
                "SELECT name FROM entities WHERE kind = 'person' AND mentions >= 2 AND normalized LIKE ?1 || ' %' AND normalized != ?1 ORDER BY mentions DESC LIMIT 2",
            )?;
            let names: Vec<String> = stmt.query_map(params![wl], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            if names.len() == 1 && !lower.contains(&names[0].to_lowercase()) && !extra.contains(&names[0]) {
                extra.push(names[0].clone());
            }
        }
        Ok(if extra.is_empty() { query.to_string() } else { format!("{query} {}", extra.join(" ")) })
    }

    pub fn entity(&self, id: i64) -> rusqlite::Result<Entity> {
        self.conn.query_row("SELECT id, kind, name, mentions, first_seen, last_seen FROM entities WHERE id = ?1", params![id], |r| {
            Ok(Entity { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)?, mentions: r.get(3)?, first_seen: r.get(4)?, last_seen: r.get(5)? })
        })
    }

    pub fn file_chunks_without_vectors(&self, limit: u32) -> rusqlite::Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, f.name || '\n' || c.text FROM file_chunks c JOIN files f ON f.id = c.file_id
             WHERE c.id NOT IN (SELECT chunk_id FROM file_vectors) ORDER BY c.id DESC LIMIT ?1",
        )?;
        let out: Vec<(i64, String)> = stmt.query_map(params![limit], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    pub fn store_file_vector(&self, chunk_id: i64, vec: &[f32]) -> rusqlite::Result<()> {
        let bytes: Vec<u8> = vec.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.conn.execute("INSERT OR REPLACE INTO file_vectors(chunk_id, vec) VALUES (?1, ?2)", params![chunk_id, bytes])?;
        if let Some(idx) = self.findex.borrow_mut().as_mut() {
            idx.push(chunk_id, vec);
        }
        Ok(())
    }

    fn file_vector_search(&self, query: &[f32], limit: usize) -> rusqlite::Result<Vec<(i64, f32)>> {
        if self.findex.borrow().is_none() {
            let mut stmt = self.conn.prepare("SELECT chunk_id, vec FROM file_vectors")?;
            let mut idx: Option<QuantIndex> = None;
            for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))? {
                let (id, bytes) = row?;
                let v = bytes_to_f32(&bytes);
                idx.get_or_insert_with(|| QuantIndex::new(v.len())).push(id, &v);
            }
            *self.findex.borrow_mut() = Some(idx.unwrap_or_else(|| QuantIndex::new(0)));
        }
        let idx = self.findex.borrow();
        // Chunks whose file was removed are dropped by the caller's join.
        Ok(idx.as_ref().map(|i| i.search(query, limit * 2)).unwrap_or_default())
    }

    /// Hybrid search over document chunks, one hit per file (its best chunk).
    pub fn search_files(&self, query: &str, query_vec: Option<&[f32]>, limit: u32) -> rusqlite::Result<Vec<FileHit>> {
        let pool = (limit as usize) * 6;
        let words: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"*", w.to_lowercase()))
            .collect();
        let mut fused: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
        if !words.is_empty() {
            let mut stmt = self.conn.prepare("SELECT rowid FROM file_chunks_fts WHERE file_chunks_fts MATCH ?1 ORDER BY bm25(file_chunks_fts) LIMIT ?2")?;
            let mut ids: Vec<i64> = stmt.query_map(params![words.join(" "), pool as i64], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            if ids.is_empty() && words.len() > 1 {
                ids = stmt.query_map(params![words.join(" OR "), pool as i64], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            }
            // File names count too: every query word (3+ letters) must appear
            // in the name, with _ and - treated as spaces.
            let name_words: Vec<String> = query
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.chars().count() >= 3)
                .map(|w| w.to_lowercase())
                .collect();
            let mut named: Vec<i64> = Vec::new();
            if !name_words.is_empty() {
                let conds = (0..name_words.len()).map(|i| format!("instr(replace(replace(lower(f.name), '_', ' '), '-', ' '), ?{}) > 0", i + 1)).collect::<Vec<_>>().join(" AND ");
                let sql = format!("SELECT c.id FROM files f JOIN file_chunks c ON c.file_id = f.id AND c.ord = 0 WHERE {conds} ORDER BY f.mtime DESC LIMIT 20");
                let mut by_name = self.conn.prepare(&sql)?;
                named = by_name.query_map(rusqlite::params_from_iter(name_words.iter()), |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            }
            for (rank, id) in named.into_iter().chain(ids.into_iter()).enumerate() {
                *fused.entry(id).or_default() += 1.0 / (60.0 + rank as f64);
            }
        }
        if let Some(q) = query_vec {
            for (rank, (id, score)) in self.file_vector_search(q, pool)?.into_iter().enumerate() {
                if score < 0.35 {
                    break;
                }
                *fused.entry(id).or_default() += 1.0 / (60.0 + rank as f64);
            }
        }
        let mut ranked: Vec<(i64, f64)> = fused.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.path, f.name, f.ext, f.mtime, f.size, substr(c.text, 1, 400), f.url FROM file_chunks c JOIN files f ON f.id = c.file_id WHERE c.id = ?1",
        )?;
        let mut seen = std::collections::HashSet::new();
        let mut hits = Vec::new();
        for (chunk_id, _) in ranked {
            let hit = stmt
                .query_row(params![chunk_id], |r| {
                    Ok(FileHit { file_id: r.get(0)?, path: r.get(1)?, name: r.get(2)?, ext: r.get(3)?, mtime: r.get(4)?, size: r.get(5)?, snippet: r.get(6)?, chunk_id, url: r.get(7)? })
                })
                .optional()?;
            if let Some(h) = hit {
                if seen.insert(h.file_id) {
                    hits.push(h);
                    if hits.len() >= limit as usize {
                        break;
                    }
                }
            }
        }
        Ok(hits)
    }

    pub fn file_chunk_text(&self, chunk_id: i64, max_chars: usize) -> rusqlite::Result<String> {
        self.conn.query_row("SELECT text FROM file_chunks WHERE id = ?1", params![chunk_id], |r| r.get::<_, String>(0)).map(|t| t.chars().take(max_chars).collect())
    }

    pub fn file_stats(&self) -> rusqlite::Result<FileStats> {
        Ok(FileStats {
            files: self.conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?,
            chunks: self.conn.query_row("SELECT COUNT(*) FROM file_chunks", [], |r| r.get(0))?,
            unembedded: self.conn.query_row("SELECT COUNT(*) FROM file_chunks WHERE id NOT IN (SELECT chunk_id FROM file_vectors)", [], |r| r.get(0))?,
            last_indexed_at: self.conn.query_row("SELECT COALESCE(MAX(indexed_at), 0) FROM files", [], |r| r.get(0))?,
            by_ext: {
                let mut st = self.conn.prepare(
                    "SELECT LOWER(COALESCE(NULLIF(ext, ''), 'other')) AS e, COUNT(*) AS n \
                     FROM files GROUP BY e ORDER BY n DESC, e LIMIT 8",
                )?;
                let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            },
        })
    }

    // ── Facts, pins, edits ─────────────────────────────────────────────────

    fn fact_row(r: &rusqlite::Row) -> rusqlite::Result<Fact> {
        Ok(Fact { id: r.get(0)?, memory_id: r.get(1)?, subject: r.get(2)?, attribute: r.get(3)?, value: r.get(4)?, as_of: r.get(5)?, origin: r.get(6)?, owner: r.get(7)?, stance: r.get(8)?, conflicts: Vec::new() })
    }

    const FACT_COLS: &'static str = "f.id, f.memory_id, f.subject, f.attribute, f.value, f.as_of, f.origin, f.owner, f.stance";

    /// Other active facts with the same key and a different value.
    fn fact_conflicts(&self, fact: &Fact) -> rusqlite::Result<Vec<FactRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.memory_id, f.value, f.as_of, m.title, m.activity_id FROM facts f JOIN memories m ON m.id = f.memory_id
             WHERE f.key = ?1 AND f.status = 'active' AND f.id != ?2 AND lower(trim(f.value)) != lower(trim(?3))
             ORDER BY f.as_of DESC LIMIT 4",
        )?;
        let out = stmt
            .query_map(params![fact_key(&fact.subject, &fact.attribute), fact.id, fact.value], |r| {
                Ok(FactRef { fact_id: r.get(0)?, memory_id: r.get(1)?, value: r.get(2)?, as_of: r.get(3)?, title: r.get(4)?, activity_id: r.get(5)? })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// Active facts of these memories, with their conflicts.
    pub fn facts_for(&self, memory_ids: &[i64]) -> rusqlite::Result<Vec<Fact>> {
        let mut out = Vec::new();
        let sql = format!("SELECT {} FROM facts f WHERE f.memory_id = ?1 AND f.status = 'active' ORDER BY f.origin DESC, f.id", Self::FACT_COLS);
        let mut stmt = self.conn.prepare(&sql)?;
        for id in memory_ids {
            let rows: Vec<Fact> = stmt.query_map(params![id], Self::fact_row)?.collect::<rusqlite::Result<_>>()?;
            for mut f in rows {
                f.conflicts = self.fact_conflicts(&f)?;
                out.push(f);
            }
        }
        Ok(out)
    }

    /// Fill `facts` on grouped cards (one query per group).
    pub fn attach_facts(&self, cards: &mut [MemoryCard]) -> rusqlite::Result<()> {
        for c in cards.iter_mut() {
            c.facts = self.facts_for(&c.ids)?;
            c.facts.truncate(12);
        }
        Ok(())
    }

    /// Facts matching a question, newest first, with the memory they came from.
    pub fn search_facts(&self, query: &str, limit: u32) -> rusqlite::Result<Vec<(Fact, i64, String)>> {
        // Question words carry no signal in a facts index ("what is the…"),
        // and with OR they let every fact match; only content words count,
        // all of them first, any of them as the fallback.
        const STOP: &[&str] = &["what", "which", "when", "where", "who", "whom", "whose", "how", "why", "the", "and", "for", "with", "from", "that", "this", "these", "those", "did", "does", "was", "were", "are", "have", "has", "had", "about", "any", "anything", "much", "many", "you", "your", "our", "into", "onto", "than", "then", "there", "here", "some", "all", "also", "just", "still", "yet", "not", "one", "two"];
        let words: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() > 2 && !STOP.contains(&w.to_lowercase().as_str()))
            .map(|w| format!("\"{}\"*", w.to_lowercase()))
            .collect();
        if words.is_empty() {
            return Ok(vec![]);
        }
        let sql = format!(
            "SELECT {}, m.activity_id, m.title FROM facts_fts x JOIN facts f ON f.id = x.rowid JOIN memories m ON m.id = f.memory_id
             WHERE facts_fts MATCH ?1 AND f.status = 'active' ORDER BY bm25(facts_fts), f.as_of DESC LIMIT ?2",
            Self::FACT_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows: Vec<(Fact, i64, String)> = stmt
            .query_map(params![words.join(" AND "), limit], |r| Ok((Self::fact_row(r)?, r.get(9)?, r.get(10)?)))?
            .collect::<rusqlite::Result<_>>()?;
        if rows.len() < limit as usize && words.len() > 1 {
            let more: Vec<(Fact, i64, String)> = stmt
                .query_map(params![words.join(" OR "), limit], |r| Ok((Self::fact_row(r)?, r.get(9)?, r.get(10)?)))?
                .collect::<rusqlite::Result<_>>()?;
            for m in more {
                if rows.len() >= limit as usize {
                    break;
                }
                if !rows.iter().any(|(f, _, _)| f.id == m.0.id) {
                    rows.push(m);
                }
            }
        }
        let mut out = Vec::new();
        for (mut f, aid, title) in rows {
            f.conflicts = self.fact_conflicts(&f)?;
            out.push((f, aid, title));
        }
        Ok(out)
    }

    /// Every pair of active facts that disagree, newest first.
    pub fn conflicting_facts(&self, limit: u32) -> rusqlite::Result<Vec<Fact>> {
        let sql = format!(
            "SELECT {} FROM facts f WHERE f.status = 'active' AND f.key IN (
                 SELECT key FROM facts WHERE status = 'active' GROUP BY key HAVING COUNT(DISTINCT lower(trim(value))) > 1)
             ORDER BY f.key, f.as_of DESC LIMIT ?1",
            Self::FACT_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows: Vec<Fact> = stmt.query_map(params![limit], Self::fact_row)?.collect::<rusqlite::Result<_>>()?;
        let mut out = Vec::new();
        for mut f in rows {
            f.conflicts = self.fact_conflicts(&f)?;
            if !f.conflicts.is_empty() {
                out.push(f);
            }
        }
        Ok(out)
    }

    pub fn add_fact(&self, memory_id: i64, subject: &str, attribute: &str, value: &str, at: i64) -> rusqlite::Result<i64> {
        let as_of: i64 = self.conn.query_row("SELECT a.started_at FROM memories m JOIN activities a ON a.id = m.activity_id WHERE m.id = ?1", params![memory_id], |r| r.get(0))?;
        self.conn.execute(
            "INSERT INTO facts(memory_id, subject, attribute, value, key, as_of, origin, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'user', 'active', ?7)",
            params![memory_id, subject.trim(), attribute.trim(), value.trim(), fact_key(subject, attribute), as_of, at],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// The user says a fact is wrong. A retracted model fact stays (so it
    /// is not re-extracted as new) but is invisible everywhere.
    pub fn retract_fact(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE facts SET status = 'retracted', origin = 'user' WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Correct a fact's value: the model's row is retracted and a user row
    /// takes its place.
    pub fn correct_fact(&self, id: i64, value: &str, at: i64) -> rusqlite::Result<i64> {
        let (memory_id, subject, attribute): (i64, String, String) =
            self.conn.query_row("SELECT memory_id, subject, attribute FROM facts WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        self.retract_fact(id)?;
        self.add_fact(memory_id, &subject, &attribute, value, at)
    }

    pub fn set_pinned(&self, ids: &[i64], pinned: bool) -> rusqlite::Result<()> {
        for id in ids {
            self.conn.execute("UPDATE memories SET pinned = ?2 WHERE id = ?1", params![id, pinned as i64])?;
        }
        Ok(())
    }

    pub fn pinned_ids(&self) -> rusqlite::Result<std::collections::HashSet<i64>> {
        let mut stmt = self.conn.prepare("SELECT id FROM memories WHERE pinned = 1")?;
        let ids: std::collections::HashSet<i64> = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// The user's own words replace the model's. Entities are re-linked.
    #[allow(clippy::too_many_arguments)]
    pub fn update_memory(&self, id: i64, title: &str, summary: &str, people: &[String], organizations: &[String], projects: &[String], decisions: &[String], at: i64) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE memories SET title = ?2, summary = ?3, people = ?4, organizations = ?5, projects = ?6, decisions = ?7, edited_at = ?8, keep = 1 WHERE id = ?1",
            params![
                id, title.trim(), summary.trim(),
                serde_json::to_string(people).unwrap_or_default(),
                serde_json::to_string(organizations).unwrap_or_default(),
                serde_json::to_string(projects).unwrap_or_default(),
                serde_json::to_string(decisions).unwrap_or_default(),
                at
            ],
        )?;
        let seen_at: i64 = tx.query_row("SELECT a.started_at FROM memories m JOIN activities a ON a.id = m.activity_id WHERE m.id = ?1", params![id], |r| r.get(0))?;
        tx.execute("DELETE FROM memory_entities WHERE memory_id = ?1", params![id])?;
        for (kind, names) in [("person", people), ("org", organizations), ("project", projects)] {
            for name in names {
                let norm = normalize_entity(name);
                if norm.chars().count() < 2 {
                    continue;
                }
                tx.execute(
                    "INSERT INTO entities(kind, name, normalized, first_seen, last_seen, mentions) VALUES (?1, ?2, ?3, ?4, ?4, 1)
                     ON CONFLICT(kind, normalized) DO UPDATE SET last_seen = MAX(last_seen, excluded.last_seen), first_seen = MIN(first_seen, excluded.first_seen)",
                    params![kind, name.trim(), norm, seen_at],
                )?;
                let eid: i64 = tx.query_row("SELECT id FROM entities WHERE kind = ?1 AND normalized = ?2", params![kind, norm], |r| r.get(0))?;
                tx.execute("INSERT OR IGNORE INTO memory_entities(memory_id, entity_id) VALUES (?1, ?2)", params![id, eid])?;
            }
        }
        // The vector for this memory is stale; it is re-embedded.
        tx.execute("DELETE FROM memory_vectors WHERE memory_id = ?1", params![id])?;
        tx.commit()?;
        self.invalidate_indexes();
        Ok(())
    }

    pub fn memory_by_id(&self, id: i64) -> rusqlite::Result<Option<MemoryCard>> {
        let sql = format!("SELECT {} FROM memories m JOIN activities a ON a.id = m.activity_id WHERE m.id = ?1", Self::MEMORY_COLS);
        self.conn.query_row(&sql, params![id], Self::row_to_memory).optional()
    }

    // ── Retention and forgetting ──────────────────────────────────────────

    /// Delete raw screen text older than `before` once its memory exists.
    /// Activities in `exempt_apps` (meetings, notes, connectors) keep theirs.
    pub fn prune_raw(&self, before: i64, exempt_apps: &[String]) -> rusqlite::Result<usize> {
        let placeholders = std::iter::repeat("?").take(exempt_apps.len()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "DELETE FROM snapshots WHERE activity_id IN (
                 SELECT id FROM activities WHERE started_at < ?1 AND memory_status IN ('done', 'skipped')
                 {} AND COALESCE(url, '') NOT LIKE 'file://%')",
            if exempt_apps.is_empty() { String::new() } else { format!("AND app_name NOT IN ({placeholders})") }
        );
        let mut params: Vec<rusqlite::types::Value> = vec![before.into()];
        params.extend(exempt_apps.iter().map(|a| rusqlite::types::Value::from(a.clone())));
        let n = self.conn.execute(&sql, rusqlite::params_from_iter(params))?;
        Ok(n)
    }

    /// Remove a name or word from everything: entities, memory fields,
    /// facts, tasks, raw text, briefings, indexed documents. Case-insensitive.
    pub fn forget_term(&self, term: &str) -> rusqlite::Result<ForgetReport> {
        let term = term.trim();
        if term.chars().count() < 2 {
            return Ok(ForgetReport::default());
        }
        let like = format!("%{}%", term.replace('%', "").replace('_', "\\_"));
        let mut r = ForgetReport::default();
        let tx = self.conn.unchecked_transaction()?;
        r.entities = tx.execute("DELETE FROM entities WHERE name LIKE ?1 OR normalized LIKE ?1", params![like])?;
        tx.execute("DELETE FROM entity_aliases WHERE alias LIKE ?1 OR shown LIKE ?1", params![like])?;
        tx.execute("UPDATE entities SET profile = NULL, profile_at = 0 WHERE profile LIKE ?1", params![like])?;
        // Memory fields: names removed from lists, the term blanked in prose.
        let rows: Vec<(i64, String, String, String, String, String, String)> = tx
            .prepare("SELECT id, title, summary, people, organizations, projects, decisions FROM memories WHERE title LIKE ?1 OR summary LIKE ?1 OR people LIKE ?1 OR organizations LIKE ?1 OR projects LIKE ?1 OR decisions LIKE ?1")?
            .query_map(params![like], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let scrub_list = |json: &str| -> String {
            let v: Vec<String> = serde_json::from_str(json).unwrap_or_default();
            serde_json::to_string(&v.into_iter().filter(|x| !contains_ci(x, term)).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into())
        };
        let scrub_lines = |json: &str| -> String {
            let v: Vec<String> = serde_json::from_str(json).unwrap_or_default();
            serde_json::to_string(&v.into_iter().map(|x| replace_ci(&x, term, "[forgotten]")).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into())
        };
        for (id, title, summary, people, orgs, projects, decisions) in rows {
            tx.execute(
                "UPDATE memories SET title = ?2, summary = ?3, people = ?4, organizations = ?5, projects = ?6, decisions = ?7 WHERE id = ?1",
                params![id, replace_ci(&title, term, "[forgotten]"), replace_ci(&summary, term, "[forgotten]"), scrub_list(&people), scrub_list(&orgs), scrub_list(&projects), scrub_lines(&decisions)],
            )?;
            tx.execute("DELETE FROM memory_vectors WHERE memory_id = ?1", params![id])?;
            r.memories += 1;
        }
        r.facts = tx.execute("DELETE FROM facts WHERE subject LIKE ?1 OR attribute LIKE ?1 OR value LIKE ?1", params![like])?;
        r.tasks = tx.execute("DELETE FROM tasks WHERE text LIKE ?1", params![like])?;
        let snaps: Vec<(i64, String, Option<String>)> = tx
            .prepare("SELECT id, text, raw_text FROM snapshots WHERE text LIKE ?1 OR raw_text LIKE ?1")?
            .query_map(params![like], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, text, raw) in snaps {
            tx.execute("UPDATE snapshots SET text = ?2, raw_text = ?3 WHERE id = ?1", params![id, replace_ci(&text, term, "[forgotten]"), raw.map(|x| replace_ci(&x, term, "[forgotten]"))])?;
            r.snapshots += 1;
        }
        let titles: Vec<(i64, String)> = tx
            .prepare("SELECT id, window_title FROM activities WHERE window_title LIKE ?1")?
            .query_map(params![like], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, title) in titles {
            tx.execute("UPDATE activities SET window_title = ?2 WHERE id = ?1", params![id, replace_ci(&title, term, "[forgotten]")])?;
        }
        r.recaps = tx.execute("DELETE FROM recaps WHERE text LIKE ?1", params![like])?;
        let chunks: Vec<(i64, String)> = tx
            .prepare("SELECT id, text FROM file_chunks WHERE text LIKE ?1")?
            .query_map(params![like], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, text) in chunks {
            tx.execute("UPDATE file_chunks SET text = ?2 WHERE id = ?1", params![id, replace_ci(&text, term, "[forgotten]")])?;
            tx.execute("DELETE FROM file_vectors WHERE chunk_id = ?1", params![id])?;
            r.files += 1;
        }
        tx.commit()?;
        Ok(r)
    }

    // ── Insights: what changed, what is near, who went quiet ───────────────

    /// Short, checkable observations for the briefing and Explore: time by
    /// project this week versus last, facts that changed this week,
    /// deadlines within three days, people who went quiet.
    pub fn insights(&self, now: i64) -> rusqlite::Result<Vec<String>> {
        let day = 86_400_000;
        let (today, _) = crate::engine::day_bounds(now);
        let week_ago = now - 7 * day;
        let two_weeks = now - 14 * day;
        let mut out: Vec<String> = Vec::new();

        // Projects: minutes this week vs last, by memories' projects.
        let mut stmt = self.conn.prepare(
            "SELECT m.projects, a.started_at, a.ended_at FROM memories m JOIN activities a ON a.id = m.activity_id
             WHERE a.started_at >= ?1 AND m.projects != '[]' AND ((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')",
        )?;
        let mut this: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut last: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for row in stmt.query_map(params![two_weeks], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))? {
            let (projects, start, end) = row?;
            let mins = ((end - start) / 60_000).max(1);
            for p in serde_json::from_str::<Vec<String>>(&projects).unwrap_or_default() {
                let bucket = if start >= week_ago { &mut this } else { &mut last };
                *bucket.entry(p).or_default() += mins;
            }
        }
        let mut projects: Vec<(&String, &i64)> = this.iter().collect();
        projects.sort_by(|a, b| b.1.cmp(a.1));
        for (p, mins) in projects.into_iter().take(3) {
            let prev = *last.get(p).unwrap_or(&0);
            if *mins >= 30 {
                let hours = |m: i64| format!("{:.1}h", m as f64 / 60.0);
                if prev == 0 {
                    out.push(format!("{p}: {} this week, nothing the week before.", hours(*mins)));
                } else if *mins > prev * 3 / 2 || *mins * 3 / 2 < prev {
                    out.push(format!("{p}: {} this week, {} the week before.", hours(*mins), hours(prev)));
                }
            }
        }

        // Facts that changed this week: an active fact whose key has an older, different value.
        let mut stmt = self.conn.prepare(
            "SELECT f.subject, f.attribute, f.value, o.value FROM facts f JOIN facts o ON o.key = f.key AND o.id != f.id AND o.as_of < f.as_of AND lower(trim(o.value)) != lower(trim(f.value))
             WHERE f.status = 'active' AND o.status = 'active' AND f.as_of >= ?1 ORDER BY f.as_of DESC LIMIT 4",
        )?;
        for row in stmt.query_map(params![week_ago], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))? {
            let (sub, attr, new, old) = row?;
            out.push(format!("{sub} {attr} changed: {old} → {new}."));
        }

        // Deadlines within three days, from written dates and date-like facts.
        for u in self.upcoming_dates(now, 3)?.into_iter().take(3) {
            let days = (u.when - today) / day;
            let when = if days <= 0 { "today".to_string() } else if days == 1 { "tomorrow".to_string() } else { format!("in {days} days") };
            out.push(format!("{} {when}: {}.", if u.about == "mentioned" { u.label.clone() } else { u.about.clone() }, u.title));
        }

        // People who went quiet: 3+ memories in the prior three weeks, none this week.
        let mut stmt = self.conn.prepare(
            "SELECT e.name, COUNT(*) c, MAX(a.started_at) FROM entities e JOIN memory_entities me ON me.entity_id = e.id JOIN memories m ON m.id = me.memory_id JOIN activities a ON a.id = m.activity_id
             WHERE e.kind = 'person' AND a.started_at >= ?1 AND a.started_at < ?2 GROUP BY e.id HAVING c >= 3
             AND NOT EXISTS (SELECT 1 FROM memory_entities me2 JOIN memories m2 ON m2.id = me2.memory_id JOIN activities a2 ON a2.id = m2.activity_id WHERE me2.entity_id = e.id AND a2.started_at >= ?2)
             ORDER BY c DESC LIMIT 3",
        )?;
        for row in stmt.query_map(params![now - 28 * day, week_ago], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))? {
            let (name, c, last_seen) = row?;
            out.push(format!("{name} came up {c} times in the weeks before, nothing since {}.", crate::engine::day_of(last_seen)));
        }
        out.truncate(8);
        Ok(out)
    }

    // ── Upcoming dates, gaps, explore ─────────────────────────────────────

    /// Dates written in recent memories and facts that fall in the next
    /// `days`: deadlines, meetings, launches. Parsed from the text as
    /// written; a date with no year is taken as the next occurrence.
    pub fn upcoming_dates(&self, now: i64, days: i64) -> rusqlite::Result<Vec<Upcoming>> {
        let (today, _) = crate::engine::day_bounds(now);
        let horizon = today + days * 86_400_000;
        let since = now - 120 * 86_400_000;
        let mut out: Vec<Upcoming> = Vec::new();
        let sql = format!(
            "SELECT {} FROM memories m JOIN activities a ON a.id = m.activity_id
             WHERE a.started_at >= ?1 AND ((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep') AND m.dates != '[]' ORDER BY a.started_at DESC LIMIT 600",
            Self::MEMORY_COLS
        );
        let cards: Vec<MemoryCard> = self.conn.prepare(&sql)?.query_map(params![since], Self::row_to_memory)?.collect::<rusqlite::Result<_>>()?;
        for c in &cards {
            for d in &c.dates {
                if let Some(when) = parse_written_date(d, today) {
                    if when >= today && when <= horizon {
                        out.push(Upcoming { when, label: d.clone(), title: c.title.clone(), activity_id: c.activity_id, memory_id: c.id, about: "mentioned".into() });
                    }
                }
            }
        }
        let mut stmt = self.conn.prepare(
            "SELECT f.subject, f.attribute, f.value, m.title, m.activity_id, m.id FROM facts f JOIN memories m ON m.id = f.memory_id JOIN activities a ON a.id = m.activity_id
             WHERE f.status = 'active' AND a.started_at >= ?1 AND ((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')",
        )?;
        let facts: Vec<(String, String, String, String, i64, i64)> = stmt.query_map(params![since], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<rusqlite::Result<_>>()?;
        for (subject, attribute, value, title, activity_id, memory_id) in facts {
            if let Some(when) = parse_written_date(&value, today) {
                if when >= today && when <= horizon {
                    out.push(Upcoming { when, label: value, title, activity_id, memory_id, about: format!("{subject} · {attribute}") });
                }
            }
        }
        out.sort_by(|a, b| a.when.cmp(&b.when).then(b.about.len().cmp(&a.about.len())));
        // One entry per (day, title).
        let mut seen = std::collections::HashSet::new();
        out.retain(|u| seen.insert((u.when, u.title.clone())));
        out.truncate(20);
        Ok(out)
    }

    /// People the user dealt with several times this week with nothing
    /// written down about it: no task, decision or fact in those memories.
    pub fn memory_gaps(&self, now: i64) -> rusqlite::Result<Vec<Gap>> {
        let since = now - 7 * 86_400_000;
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.name, COUNT(DISTINCT m.id) c,
                    SUM(EXISTS(SELECT 1 FROM tasks t WHERE t.memory_id = m.id)) + SUM(EXISTS(SELECT 1 FROM facts f WHERE f.memory_id = m.id AND f.status = 'active')) + SUM(m.decisions != '[]') outcomes
             FROM entities e JOIN memory_entities me ON me.entity_id = e.id JOIN memories m ON m.id = me.memory_id JOIN activities a ON a.id = m.activity_id
             WHERE e.kind = 'person' AND a.started_at >= ?1 AND ((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')
             GROUP BY e.id HAVING c >= 3 AND outcomes = 0 ORDER BY c DESC LIMIT 5",
        )?;
        let rows: Vec<(i64, String, i64)> = stmt.query_map(params![since], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, name, c)| Gap {
                kind: "person".into(),
                text: format!("{name} came up {c} times this week, but nothing is written down: no decision, task or fact."),
                entity_id: Some(id),
                question: format!("What did I discuss with {name} this week and what is still open?"),
            })
            .collect())
    }

    pub fn explore(&self, now: i64) -> rusqlite::Result<Explore> {
        let since = now - 30 * 86_400_000;
        let visible = "((m.keep = 1 AND m.feedback IS NOT 'ignore') OR m.feedback = 'keep')";
        let per_day = |sql: &str| -> rusqlite::Result<Vec<(String, i64)>> {
            let mut stmt = self.conn.prepare(sql)?;
            let rows: Vec<(i64, i64)> = stmt.query_map(params![since], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            Ok(rows.into_iter().map(|(ms, n)| (crate::engine::day_of(ms), n)).collect())
        };
        let offset = crate::engine::day_bounds(now).0 % 86_400_000;
        let memories_per_day = per_day(&format!(
            "SELECT ((a.started_at - {offset}) / 86400000) * 86400000 + {offset} d, COUNT(*) FROM memories m JOIN activities a ON a.id = m.activity_id WHERE a.started_at >= ?1 AND {visible} GROUP BY d ORDER BY d"
        ))?;
        let minutes_per_day = per_day(&format!(
            "SELECT ((started_at - {offset}) / 86400000) * 86400000 + {offset} d, SUM(ended_at - started_at) / 60000 FROM activities WHERE started_at >= ?1 GROUP BY d ORDER BY d"
        ))?;
        // Where the time went: by app, or by site when the app is a browser.
        let mut places_stmt = self.conn.prepare(
            "SELECT app_name, url, SUM(ended_at - started_at) / 60000 m FROM activities WHERE started_at >= ?1 GROUP BY app_name, url",
        )?;
        let raw: Vec<(String, Option<String>, i64)> = places_stmt
            .query_map(params![since], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut places: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut cats: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for (app, url, mins) in raw {
            if mins <= 0 { continue }
            let place = crate::clean::place_of(&app, url.as_deref());
            *places.entry(place.clone()).or_default() += mins;
            *cats.entry(crate::clean::category_of(&place).to_string()).or_default() += mins;
        }
        let mut top_places: Vec<(String, String, i64)> = places
            .into_iter()
            .map(|(p, m)| (crate::clean::category_of(&p).to_string(), p, m))
            .map(|(c, p, m)| (p, c, m))
            .collect();
        top_places.sort_by(|a, b| b.2.cmp(&a.2));
        top_places.truncate(8);
        let mut by_category: Vec<(String, i64)> = cats.into_iter().collect();
        by_category.sort_by(|a, b| b.1.cmp(&a.1));

        let mut stmt = self.conn.prepare(&format!("SELECT m.kind, COUNT(*) c FROM memories m JOIN activities a ON a.id = m.activity_id WHERE a.started_at >= ?1 AND {visible} GROUP BY m.kind ORDER BY c DESC"))?;
        let by_kind: Vec<(String, i64)> = stmt.query_map(params![since], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let top = |kind: &str| -> rusqlite::Result<Vec<Entity>> {
            let mut stmt = self.conn.prepare(
                "SELECT e.id, e.kind, e.name, COUNT(DISTINCT m.id) c, e.first_seen, e.last_seen FROM entities e JOIN memory_entities me ON me.entity_id = e.id
                 JOIN memories m ON m.id = me.memory_id JOIN activities a ON a.id = m.activity_id WHERE e.kind = ?1 AND a.started_at >= ?2 GROUP BY e.id ORDER BY c DESC LIMIT 8",
            )?;
            let rows: Vec<Entity> = stmt
                .query_map(params![kind, since], |r| Ok(Entity { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)?, mentions: r.get(3)?, first_seen: r.get(4)?, last_seen: r.get(5)? }))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        };
        let count = |sql: &str| -> rusqlite::Result<i64> { self.conn.query_row(sql, [], |r| r.get(0)) };
        Ok(Explore {
            by_category,
            top_places,
            memories_per_day,
            minutes_per_day,
            by_kind,
            top_people: top("person")?,
            top_orgs: top("org")?,
            top_projects: top("project")?,
            facts: count("SELECT COUNT(*) FROM facts WHERE status = 'active'")?,
            conflicts: count("SELECT COUNT(*) FROM (SELECT key FROM facts WHERE status = 'active' GROUP BY key HAVING COUNT(DISTINCT lower(trim(value))) > 1)")?,
            tasks_open: count("SELECT COUNT(*) FROM tasks WHERE status = 'open'")?,
            tasks_done: count("SELECT COUNT(*) FROM tasks WHERE status = 'done'")?,
            files: count("SELECT COUNT(*) FROM files")?,
            meetings: count("SELECT COUNT(*) FROM meetings WHERE status = 'done'")?,
            memories: count("SELECT COUNT(*) FROM memories")?,
            kept: count(&format!("SELECT COUNT(*) FROM memories m WHERE {visible}"))?,
            pinned: count("SELECT COUNT(*) FROM memories WHERE pinned = 1")?,
            edited: count("SELECT COUNT(*) FROM memories WHERE edited_at IS NOT NULL")?,
        })
    }

    pub fn set_memory_feedback(&self, ids: &[i64], feedback: Option<&str>) -> rusqlite::Result<()> {
        for id in ids {
            self.conn.execute("UPDATE memories SET feedback = ?2 WHERE id = ?1", params![id, feedback])?;
        }
        Ok(())
    }

    /// Training labels: every memory the user judged, with the model's
    /// verdict and the source text, as JSON lines. Contact details were
    /// already redacted at capture time.
    pub fn export_labels(&self) -> rusqlite::Result<String> {
        let mut stmt = self.conn.prepare(
            "SELECT m.activity_id, m.kind, m.title, m.summary, m.people, m.organizations, m.dates, m.numbers,
                    m.keep, m.confidence, m.model, m.feedback, a.app_name, a.window_title, a.url, a.started_at, a.ended_at, m.id
             FROM memories m JOIN activities a ON a.id = m.activity_id WHERE m.feedback IS NOT NULL ORDER BY m.id",
        )?;
        let mut out = String::new();
        let rows = stmt.query_map([], |r| {
            let activity_id: i64 = r.get(0)?;
            let list = |i: usize| -> rusqlite::Result<serde_json::Value> {
                Ok(serde_json::from_str(&r.get::<_, String>(i)?).unwrap_or(serde_json::Value::Array(vec![])))
            };
            Ok(serde_json::json!({
                "memory_id": r.get::<_, i64>(17)?,
                "activity_id": activity_id,
                "app": r.get::<_, String>(12)?,
                "window_title": r.get::<_, String>(13)?,
                "url": r.get::<_, Option<String>>(14)?,
                "started_at": r.get::<_, i64>(15)?,
                "ended_at": r.get::<_, i64>(16)?,
                "text": self.activity_text(activity_id, 4_000)?,
                "model": {
                    "name": r.get::<_, String>(10)?,
                    "keep": r.get::<_, bool>(8)?,
                    "confidence": r.get::<_, f64>(9)?,
                    "kind": r.get::<_, String>(1)?,
                    "title": r.get::<_, String>(2)?,
                    "summary": r.get::<_, String>(3)?,
                    "people": list(4)?, "organizations": list(5)?, "dates": list(6)?, "numbers": list(7)?
                },
                "label": { "keep": r.get::<_, String>(11)? == "keep" }
            }))
        })?;
        for row in rows {
            out.push_str(&row?.to_string());
            out.push('\n');
        }
        Ok(out)
    }

    /// Labels the user has given and not yet contributed (or excluded).
    pub fn unsent_label_ids(&self) -> rusqlite::Result<Vec<(i64, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, feedback FROM memories WHERE feedback IS NOT NULL AND feedback_sent_at IS NULL ORDER BY id",
        )?;
        let out: Vec<(i64, String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// Records for contribution, same shape as export_labels, only unsent.
    pub fn unsent_label_records(&self) -> rusqlite::Result<Vec<(i64, serde_json::Value)>> {
        let all = self.export_labels()?;
        let unsent: std::collections::HashSet<i64> = self.unsent_label_ids()?.into_iter().map(|(id, _, _)| id).collect();
        let mut out = Vec::new();
        for line in all.lines() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
            if let Some(mid) = v["memory_id"].as_i64() {
                if unsent.contains(&mid) {
                    out.push((mid, v));
                }
            }
        }
        Ok(out)
    }

    /// `at` = when sent; -1 = the user chose never to send this one.
    pub fn mark_labels_sent(&self, ids: &[i64], at: i64) -> rusqlite::Result<()> {
        for id in ids {
            self.conn.execute("UPDATE memories SET feedback_sent_at = ?2 WHERE id = ?1", params![id, at])?;
        }
        Ok(())
    }

    pub fn memory_counts(&self) -> rusqlite::Result<MemoryCounts> {
        Ok(MemoryCounts {
            pending: self.conn.query_row(
                "SELECT COUNT(*) FROM activities WHERE memory_status = 'pending' AND snapshot_count > 0",
                [],
                |r| r.get(0),
            )?,
            memories: self.conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?,
            kept: self.conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE (keep = 1 AND feedback IS NOT 'ignore') OR feedback = 'keep'",
                [],
                |r| r.get(0),
            )?,
        })
    }

    /// Delete every activity recorded for these app names (case-insensitive).
    pub fn purge_apps(&self, names: &[&str]) -> rusqlite::Result<usize> {
        self.invalidate_indexes();
        let mut total = 0;
        for name in names {
            total += self.conn.execute("DELETE FROM activities WHERE app_name = ?1 COLLATE NOCASE", params![name])?;
        }
        Ok(total)
    }

    pub fn delete_activity(&self, id: i64) -> rusqlite::Result<bool> {
        self.invalidate_indexes();
        Ok(self.conn.execute("DELETE FROM activities WHERE id = ?1", params![id])? > 0)
    }

    pub fn wipe(&self) -> rusqlite::Result<()> {
        self.invalidate_indexes();
        self.conn.execute_batch(
            "DELETE FROM meetings; DELETE FROM files; DELETE FROM tasks; DELETE FROM memory_entities; DELETE FROM entities; DELETE FROM memories; DELETE FROM snapshots; DELETE FROM activities; DELETE FROM source_lines;
             INSERT INTO file_chunks_fts(file_chunks_fts) VALUES ('rebuild');
             INSERT INTO memories_fts(memories_fts) VALUES ('rebuild');
             INSERT INTO snapshots_fts(snapshots_fts) VALUES ('rebuild');
             INSERT INTO activities_fts(activities_fts) VALUES ('rebuild');
             VACUUM;",
        )
    }

    pub fn stats(&self, today_start_ms: i64) -> rusqlite::Result<Stats> {
        let activities = self.conn.query_row("SELECT COUNT(*) FROM activities", [], |r| r.get(0))?;
        let snapshots = self.conn.query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))?;
        let activities_today = self.conn.query_row(
            "SELECT COUNT(*) FROM activities WHERE started_at >= ?1",
            params![today_start_ms],
            |r| r.get(0),
        )?;
        let db_size_bytes = self
            .path
            .as_ref()
            .map(|p| {
                ["", "-wal", "-shm"]
                    .iter()
                    .filter_map(|suffix| std::fs::metadata(format!("{}{}", p.display(), suffix)).ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0);
        Ok(Stats { activities, snapshots, activities_today, db_size_bytes })
    }

    #[cfg(test)]
    fn fts_integrity_ok(&self) -> bool {
        self.conn
            .execute_batch(
                "INSERT INTO snapshots_fts(snapshots_fts) VALUES ('integrity-check');
                 INSERT INTO activities_fts(activities_fts) VALUES ('integrity-check');
                 INSERT INTO memories_fts(memories_fts) VALUES ('integrity-check');
                 INSERT INTO file_chunks_fts(file_chunks_fts) VALUES ('integrity-check');",
            )
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(store: &Store) -> (i64, i64) {
        let a = store.start_activity("Google Chrome", None, "Stripe pricing", Some("https://stripe.com/pricing"), 1_000).unwrap();
        store.add_snapshot(a, "Stripe charges 2.9% plus 30 cents per successful card charge", 1_000).unwrap();
        let b = store.start_activity("Notes", None, "Tender draft", None, 2_000).unwrap();
        store.add_snapshot(b, "Procurement tender for road maintenance, deadline 12 October", 2_000).unwrap();
        (a, b)
    }

    #[test]
    fn finds_by_body_text_and_title() {
        let store = Store::open_in_memory().unwrap();
        let (a, b) = seed(&store);
        assert_eq!(store.search("card charge", 10).unwrap()[0].activity.id, a);
        assert_eq!(store.search("tender", 10).unwrap()[0].activity.id, b);
        assert_eq!(store.search("stripe.com", 10).unwrap()[0].activity.id, a);
        // porter stemming + prefix match
        assert_eq!(store.search("procure", 10).unwrap()[0].activity.id, b);
    }

    #[test]
    fn snippet_has_highlight_markers() {
        let store = Store::open_in_memory().unwrap();
        seed(&store);
        let hit = &store.search("deadline", 10).unwrap()[0];
        assert!(hit.snippet.contains(HL_START) && hit.snippet.contains(HL_END));
    }

    #[test]
    fn delete_keeps_index_healthy() {
        let store = Store::open_in_memory().unwrap();
        let (a, b) = seed(&store);
        assert!(store.delete_activity(a).unwrap());
        assert!(store.fts_integrity_ok(), "FTS index must survive deletes");
        assert!(store.search("stripe", 10).unwrap().is_empty());
        assert_eq!(store.search("tender", 10).unwrap()[0].activity.id, b);
        // cascade removed the snapshot too
        let n: i64 = store.conn.query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn title_update_reindexes() {
        let store = Store::open_in_memory().unwrap();
        let (_, b) = seed(&store);
        store.conn.execute("UPDATE activities SET window_title = 'Budget memo' WHERE id = ?1", params![b]).unwrap();
        store.extend_activity(b, 9_000).unwrap();
        assert!(store.fts_integrity_ok());
        assert_eq!(store.search("budget memo", 10).unwrap()[0].activity.id, b);
        assert!(store.search("draft", 10).unwrap().is_empty());
    }

    #[test]
    fn hostile_queries_do_not_error() {
        let store = Store::open_in_memory().unwrap();
        seed(&store);
        for q in ["\"", "AND OR NOT", "*", "a\"b", "NEAR(", "   ", "stripe\"* OR"] {
            assert!(store.search(q, 10).is_ok(), "query {q:?} errored");
        }
    }

    #[test]
    fn falls_back_to_any_word() {
        let store = Store::open_in_memory().unwrap();
        let (a, _) = seed(&store);
        let hits = store.search("stripe unicorns", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].activity.id, a);
    }

    #[test]
    fn purge_removes_named_apps_only() {
        let store = Store::open_in_memory().unwrap();
        let (_, b) = seed(&store);
        let own = store.start_activity("rat-mac", None, "Lane", None, 3_000).unwrap();
        store.add_snapshot(own, "Delete this memory", 3_000).unwrap();
        assert_eq!(store.purge_apps(&["Lane", "RAT-MAC", "Google Chrome"]).unwrap(), 2);
        assert!(store.fts_integrity_ok());
        assert_eq!(store.list_activities(None, 10).unwrap().iter().map(|a| a.id).collect::<Vec<_>>(), vec![b]);
    }

    #[test]
    fn trivial_activities_are_discarded() {
        let store = Store::open_in_memory().unwrap();
        let id = store.start_activity("Finder", None, "", None, 0).unwrap();
        store.extend_activity(id, 1_500).unwrap();
        assert!(store.discard_if_trivial(id, 3_000).unwrap());
        assert!(store.fts_integrity_ok());
    }

    #[test]
    fn repeated_chrome_is_dropped_after_it_is_seen_twice() {
        let store = Store::open_in_memory().unwrap();
        let chrome = "Inbox\nStarred\nSent\n";
        let mut ids = Vec::new();
        for (i, body) in ["First email about the tender deadline", "Second email about the budget figures", "Third email about travel"].iter().enumerate() {
            let a = store.start_activity("Google Chrome", None, "Mail", Some(&format!("https://mail.google.com/mail/#inbox/{i}")), i as i64 * 1000).unwrap();
            ids.push(store.add_snapshot(a, &format!("{chrome}{body}"), i as i64 * 1000).unwrap().unwrap());
        }
        let texts: Vec<String> = ids
            .iter()
            .map(|id| store.conn.query_row("SELECT text FROM snapshots WHERE id = ?1", params![id], |r| r.get(0)).unwrap())
            .collect();
        assert!(texts[0].starts_with("Inbox"), "first sighting is kept");
        assert!(texts[1].starts_with("Inbox"), "second sighting is kept");
        assert_eq!(texts[2], "Third email about travel", "from the third on it is chrome");
        assert!(store.search("starred", 10).unwrap().len() == 2);
    }

    #[test]
    fn empty_or_unchanged_snapshots_are_not_stored() {
        let store = Store::open_in_memory().unwrap();
        let a = store.start_activity("Notes", None, "Plan", None, 0).unwrap();
        assert!(store.add_snapshot(a, "\u{e000}\n□\n", 0).unwrap().is_none(), "glyph-only text");
        assert!(store.add_snapshot(a, "Real note", 1).unwrap().is_some());
        assert!(store.add_snapshot(a, "Real note\n\u{e5cd}", 2).unwrap().is_none(), "same clean text");
        assert_eq!(store.stats(0).unwrap().snapshots, 1);
    }

    #[test]
    fn reclean_migrates_old_rows_and_reports_savings() {
        let store = Store::open_in_memory().unwrap();
        let a = store.start_activity("Google Chrome", None, "Mail", Some("https://mail.google.com/"), 0).unwrap();
        // Simulate rows written by a build without cleanup: raw_text NULL.
        for i in 0..3 {
            store
                .conn
                .execute(
                    "INSERT INTO snapshots(activity_id, captured_at, text, text_hash) VALUES (?1, ?2, ?3, ?4)",
                    params![a, i, format!("Inbox\nStarred\nmessage {i}"), format!("h{i}")],
                )
                .unwrap();
        }
        store.conn.execute("UPDATE activities SET snapshot_count = 3 WHERE id = ?1", params![a]).unwrap();
        assert!(store.needs_reclean().unwrap());
        let stats = store.reclean_all().unwrap();
        assert!(!store.needs_reclean().unwrap());
        assert_eq!(stats.snapshots, 3);
        assert!(stats.clean_chars < stats.raw_chars);
        assert!(store.fts_integrity_ok());
        let last: String = store.conn.query_row("SELECT text FROM snapshots ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap();
        assert_eq!(last, "message 2");
        let detail = store.get_activity(a).unwrap().unwrap();
        assert!(detail.snapshots[2].raw.contains("Starred"), "raw text preserved");
    }

    #[test]
    fn memory_pipeline_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let (a, b) = seed(&store);
        store.extend_activity(a, 5_000).unwrap();
        store.extend_activity(b, 6_000).unwrap();
        // Nothing is pending while activities are still fresh.
        assert!(store.next_pending_activity(3_000).unwrap().is_none());
        let next = store.next_pending_activity(10_000).unwrap().unwrap();
        assert_eq!(next.id, a, "oldest first");
        assert!(store.activity_text(a, 4_000).unwrap().contains("2.9%"));
        let m = NewMemory {
            kind: "article".into(),
            title: "Stripe pricing".into(),
            summary: "Stripe charges 2.9% + 30¢ per card charge.".into(),
            numbers: vec!["2.9%".into()],
            keep: true,
            confidence: 0.9,
            model: "test".into(),
            ..Default::default()
        };
        let id = store.insert_memory(a, &m, 7_000).unwrap();
        assert_eq!(store.next_pending_activity(10_000).unwrap().unwrap().id, b, "a is done");
        let cards = store.list_memories(Some("stripe"), true, 10).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].numbers, vec!["2.9%"]);
        assert_eq!(cards[0].app_name, "Google Chrome");
        // User overrules the model; reprocessing keeps the verdict.
        store.set_memory_feedback(&[id], Some("ignore")).unwrap();
        assert!(store.list_memories(None, true, 10).unwrap().is_empty());
        store.insert_memory(a, &m, 8_000).unwrap();
        assert_eq!(store.list_memories(None, false, 10).unwrap()[0].feedback.as_deref(), Some("ignore"));
        let c = store.memory_counts().unwrap();
        assert_eq!((c.pending, c.memories, c.kept), (1, 1, 0));
        let labels = store.export_labels().unwrap();
        assert_eq!(labels.lines().count(), 1);
        assert_eq!(store.unsent_label_ids().unwrap().len(), 1);
        let recs = store.unsent_label_records().unwrap();
        assert_eq!(recs.len(), 1);
        store.mark_labels_sent(&[recs[0].0], 99).unwrap();
        assert!(store.unsent_label_ids().unwrap().is_empty());
        let row: serde_json::Value = serde_json::from_str(labels.lines().next().unwrap()).unwrap();
        assert_eq!(row["label"]["keep"], false);
        assert_eq!(row["model"]["keep"], true);
        assert!(row["text"].as_str().unwrap().contains("2.9%"));
        // A new snapshot reopens the activity for processing.
        store.add_snapshot(a, "Stripe raised fees to 3.1% for international cards", 9_000).unwrap();
        assert_eq!(store.memory_counts().unwrap().pending, 2);
        assert!(store.delete_activity(a).unwrap());
        assert!(store.fts_integrity_ok());
        assert!(store.list_memories(None, false, 10).unwrap().is_empty(), "memory deleted with its activity");
    }

    #[test]
    fn sessions_of_the_same_page_merge_into_one_card() {
        let store = Store::open_in_memory().unwrap();
        let url = Some("https://claude.ai/chat/395d");
        let mut ids = Vec::new();
        for (i, (start, len)) in [(0i64, 600_000i64), (900_000, 1_800_000), (3_000_000, 300_000), (30_000_000, 600_000)].iter().enumerate() {
            let a = store.start_activity("Google Chrome", None, "Vatsalya - Claude", url, *start).unwrap();
            store.extend_activity(a, start + len).unwrap();
            let m = NewMemory { kind: "chat".into(), title: format!("Session {i}"), summary: "s".into(), people: vec![format!("P{i}")], keep: true, confidence: 0.9, model: "t".into(), ..Default::default() };
            ids.push(store.insert_memory(a, &m, start + len).unwrap());
        }
        let cards = store.list_memories(None, true, 50).unwrap();
        assert_eq!(cards.len(), 2, "three close sessions merge; the one 7.5 h later is separate");
        let big = cards.iter().find(|c| c.sessions == 3).unwrap();
        assert_eq!(big.total_ms, 2_700_000);
        assert_eq!(big.title, "Session 1", "longest session names the card");
        assert_eq!(big.people.len(), 3);
        assert_eq!(big.ids.len(), 3);
        store.set_memory_feedback(&big.ids, Some("ignore")).unwrap();
        assert_eq!(store.list_memories(None, true, 50).unwrap().len(), 1);
        assert_eq!(store.list_memories(Some("session"), false, 50).unwrap().len(), 2, "search results group too");
    }

    #[test]
    fn vector_search_and_hybrid_fusion() {
        let store = Store::open_in_memory().unwrap();
        let mut ids = Vec::new();
        for (i, (title, vec)) in [("Stripe fees", [1.0f32, 0.0, 0.0]), ("Tender deadline", [0.0, 1.0, 0.0]), ("Budget memo", [0.7, 0.7, 0.0])].iter().enumerate() {
            let a = store.start_activity("Notes", None, title, None, i as i64 * 10_000).unwrap();
            store.extend_activity(a, i as i64 * 10_000 + 5_000).unwrap();
            let m = NewMemory { kind: "document".into(), title: title.to_string(), summary: "x".into(), keep: true, confidence: 0.9, model: "t".into(), ..Default::default() };
            let id = store.insert_memory(a, &m, 0).unwrap();
            store.store_vector(id, "test", vec).unwrap();
            ids.push(id);
        }
        assert!(store.memories_without_vectors(10).unwrap().is_empty());
        assert!(store.embedding_text(ids[0]).unwrap().starts_with("search_document: Stripe fees"));
        let top = store.vector_search(&[0.9, 0.1, 0.0], 2).unwrap();
        assert_eq!(top[0].0, ids[0]);
        // "budget" matches by keyword only; the vector points at Stripe. Both surface.
        let hits = store.search_memories("budget", Some(&[1.0, 0.0, 0.0]), true, 10).unwrap();
        let titles: Vec<_> = hits.iter().map(|h| h.title.as_str()).collect();
        assert!(titles.contains(&"Budget memo") && titles.contains(&"Stripe fees"), "{titles:?}");
        assert_eq!(store.search_memories("unicorns", None, true, 10).unwrap().len(), 0);
        store.delete_activity(1).unwrap();
        assert_eq!(store.vector_search(&[1.0, 0.0, 0.0], 5).unwrap().len(), 2, "vector removed with its memory");
    }

    #[test]
    fn entities_tasks_and_graph() {
        let store = Store::open_in_memory().unwrap();
        let a = store.start_activity("Mail", None, "Re: tender", None, 1_000).unwrap();
        store.extend_activity(a, 2_000).unwrap();
        let b = store.start_activity("Slack", None, "#phoenix", None, 3_000).unwrap();
        store.extend_activity(b, 4_000).unwrap();
        let m1 = NewMemory {
            kind: "email".into(), title: "Tender".into(), summary: "s".into(), keep: true, confidence: 0.9, model: "t".into(),
            people: vec!["Sarah Khan".into(), "Tom".into()], organizations: vec!["Acme Corp.".into()], projects: vec!["Phoenix".into()],
            tasks: vec!["Send the revised tender to Sarah by Friday".into()], ..Default::default()
        };
        let m2 = NewMemory {
            kind: "chat".into(), title: "Phoenix standup".into(), summary: "s".into(), keep: true, confidence: 0.9, model: "t".into(),
            people: vec!["sarah khan".into()], projects: vec!["phoenix".into()], tasks: vec!["Tom to fix auth by Monday".into()], ..Default::default()
        };
        let id1 = store.insert_memory(a, &m1, 2_000).unwrap();
        store.insert_memory(b, &m2, 4_000).unwrap();
        let people = store.list_entities(Some("person"), None, 10).unwrap();
        assert_eq!(people.len(), 2, "Sarah Khan and sarah khan are one person");
        assert_eq!((people[0].name.as_str(), people[0].mentions), ("Sarah Khan", 2));
        assert_eq!(store.list_entities(None, Some("acme"), 10).unwrap()[0].name, "Acme Corp.");
        assert_eq!(store.entity_memories(people[0].id, 10).unwrap().len(), 2);
        let g = store.graph(50, 1).unwrap();
        assert_eq!(g.nodes.len(), 4);
        let sarah = people[0].id;
        let phoenix = store.list_entities(Some("project"), None, 1).unwrap()[0].id;
        assert!(g.edges.iter().any(|e| (e.source == sarah.min(phoenix)) && e.target == sarah.max(phoenix) && e.weight == 2));
        let open = store.list_tasks("open", 10).unwrap();
        assert_eq!(open.len(), 2);
        assert_eq!(store.search_tasks("tender sarah", 10).unwrap().len(), 1);
        assert_eq!(store.entities_named_in("what did sarah say about phoenix", 5).unwrap().len(), 2);
        let brief = store.entity_brief(people[0].id, 5).unwrap();
        assert!(brief.starts_with("Person: Sarah Khan · mentioned 2 times"), "{brief}");
        assert!(brief.contains("Phoenix (2×)") && brief.contains("- Tender —"), "{brief}");
        let t = open.iter().find(|t| t.text.starts_with("Send")).unwrap();
        store.set_task_status(t.id, "done", 5_000).unwrap();
        assert_eq!(store.list_tasks("open", 10).unwrap().len(), 1);
        // Reprocessing the activity keeps the done task done and doesn't duplicate it.
        store.insert_memory(a, &m1, 6_000).unwrap();
        assert_eq!(store.list_tasks("open", 10).unwrap().len(), 1);
        assert_eq!(store.list_tasks("done", 10).unwrap().len(), 1);
        assert!(store.list_memories(Some("phoenix"), true, 10).unwrap().len() >= 1, "projects are searchable");
        // Thumbs-down on a memory hides its tasks.
        let m2_id = store.list_memories(Some("standup"), true, 1).unwrap()[0].ids[0];
        store.set_memory_feedback(&[m2_id], Some("ignore")).unwrap();
        assert_eq!(store.list_tasks("open", 10).unwrap().len(), 0);
        store.set_memory_feedback(&[m2_id], None).unwrap();
        store.delete_activity(a).unwrap();
        assert!(store.fts_integrity_ok());
        assert_eq!(store.list_tasks("done", 10).unwrap().len(), 0, "tasks go with their memory");
        let _ = id1;
    }

    #[test]
    fn recap_storage_and_day_queries() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.recap("2026-09-17").unwrap().is_none());
        store.save_recap("2026-09-17", "You worked on the tender.", "[1]", 5).unwrap();
        store.save_recap("2026-09-17", "Updated.", "[1,2]", 6).unwrap();
        assert_eq!(store.recap("2026-09-17").unwrap().unwrap().0, "Updated.");
        let a = store.start_activity("Mail", None, "Re: tender", None, 1_000).unwrap();
        store.extend_activity(a, 121_000).unwrap();
        let m = NewMemory { kind: "email".into(), title: "Tender".into(), summary: "s".into(), keep: true, confidence: 0.9, model: "t".into(), tasks: vec!["Send the revised tender by Friday".into()], ..Default::default() };
        store.insert_memory(a, &m, 2_000).unwrap();
        assert_eq!(store.memories_between(0, 10_000, 10).unwrap().len(), 1);
        assert_eq!(store.memories_between(10_000, 20_000, 10).unwrap().len(), 0);
        assert_eq!(store.tasks_between(0, 10_000).unwrap().len(), 1);
        assert_eq!(store.time_by_app(0, 10_000).unwrap(), vec![("Mail".to_string(), 2)]);
        let m2 = NewMemory { kind: "chat".into(), title: "Standup".into(), summary: "s".into(), keep: true, confidence: 0.9, model: "t".into(), decisions: vec!["Ship on Friday".into()], ..Default::default() };
        let b = store.start_activity("Slack", None, "standup", None, 3_000).unwrap();
        store.insert_memory(b, &m2, 4_000).unwrap();
        let d = store.decisions_between(0, 10_000).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, "Ship on Friday");
        assert!(store.list_memories(Some("friday"), true, 10).unwrap().len() == 1, "decisions are searchable");
        let n = store.create_note("Call Sarah about the tender\nShe wants it by Friday", 5_000, "Note").unwrap();
        assert_eq!(store.get_activity(n).unwrap().unwrap().activity.window_title, "Call Sarah about the tender");
        assert_eq!(store.search("tender", 10).unwrap().len(), 2);
    }

    #[test]
    fn board_positions_and_memory_nodes() {
        let store = Store::open_in_memory().unwrap();
        store.set_board_position("e:1", Some((10.0, -5.5))).unwrap();
        store.set_board_position("e:2", Some((1.0, 1.0))).unwrap();
        store.set_board_position("e:2", None).unwrap();
        assert_eq!(store.board_positions().unwrap(), vec![("e:1".to_string(), 10.0, -5.5)]);
        let a = store.start_activity("Mail", None, "Re: tender", None, 1_000).unwrap();
        let m = NewMemory { kind: "email".into(), title: "Tender".into(), summary: "s".into(), keep: true, confidence: 0.9, model: "t".into(), people: vec!["Sarah".into()], ..Default::default() };
        store.insert_memory(a, &m, 2_000).unwrap();
        let nodes = store.board_memories(0, 10).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].1.len(), 1, "linked to Sarah");
        assert!(store.board_memories(5_000, 10).unwrap().is_empty(), "time filter");
    }

    #[test]
    fn meetings_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let (id, activity) = store.create_meeting("Standup", 1_000, "/tmp/x").unwrap();
        store.add_snapshot(activity, "[00:00] You: Hi Sarah, can you send the tender?\n[00:05] Others: Yes, by Friday.", 2_000).unwrap();
        store.set_meeting_status(id, "done", "", Some(60_000)).unwrap();
        store.rename_meeting(id, "Tender sync").unwrap();
        let m = &store.list_meetings(10).unwrap()[0];
        assert_eq!((m.title.as_str(), m.status.as_str(), m.ended_at), ("Tender sync", "done", Some(60_000)));
        assert_eq!(store.search("tender", 10).unwrap()[0].activity.window_title, "Tender sync");
        assert_eq!(store.next_pending_activity(100_000).unwrap().unwrap().id, activity, "meeting becomes a memory");
    }

    #[test]
    fn files_index_and_search() {
        let store = Store::open_in_memory().unwrap();
        let chunks = vec!["Tender for road maintenance in Jorhat, deadline 12 October 2026.".to_string(), "Budget ₹35 lakh including GST.".to_string()];
        let id = store.upsert_file("/tmp/tender.pdf", 1000, 5, &chunks, 10).unwrap();
        assert!(store.file_is_current("/tmp/tender.pdf", 1000, 5).unwrap());
        assert!(!store.file_is_current("/tmp/tender.pdf", 1001, 5).unwrap());
        store.upsert_file("/tmp/notes.txt", 10, 1, &["Grocery list: milk, eggs".to_string()], 10).unwrap();
        let hits = store.search_files("jorhat tender", None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file_id, id);
        assert!(hits[0].snippet.contains("Jorhat"));
        assert_eq!(store.search_files("notes", None, 10).unwrap()[0].name, "notes.txt", "file names match too");
        store.upsert_file("/tmp/AISF_NeatHub_Press_Release.docx", 10, 1, &["Guwahati, Assam. For immediate release.".to_string()], 10).unwrap();
        assert_eq!(store.search_files("neathub press release", None, 10).unwrap()[0].name, "AISF_NeatHub_Press_Release.docx", "per-word name match");
        assert_eq!(store.file_chunks_without_vectors(10).unwrap().len(), 4);
        // Same content again: chunks untouched (and vectors would survive).
        store.upsert_file("/tmp/tender.pdf", 1000, 6, &chunks, 11).unwrap();
        assert_eq!(store.file_stats().unwrap().chunks, 4);
        let present: std::collections::HashSet<String> = ["/tmp/tender.pdf".to_string()].into_iter().collect();
        assert_eq!(store.remove_missing_files(&present, &["/tmp".to_string()]).unwrap(), 2);
        assert_eq!(store.file_stats().unwrap().files, 1);
        assert!(store.fts_integrity_ok());
    }

    #[test]
    fn settings_from_older_versions_keep_their_values() {
        let store = Store::open_in_memory().unwrap();
        store
            .conn
            .execute(
                "INSERT INTO settings(key, value) VALUES ('settings', ?1)",
                params![r#"{"intervalSecs":9,"idleMinutes":3,"excludedApps":["Zoom"],"excludedUrlPatterns":[],"readBrowserText":false}"#],
            )
            .unwrap();
        let s = store.settings();
        assert_eq!((s.interval_secs, s.idle_minutes, s.read_browser_text), (9, 3, false));
        assert_eq!(s.excluded_apps, vec!["Zoom".to_string()]);
        assert!(s.exclude_messaging && !s.onboarding_done, "new fields take defaults");
    }

    #[test]
    fn wipe_and_settings_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        seed(&store);
        let mut s = store.settings();
        assert_eq!(s, Settings::default());
        s.interval_secs = 10;
        store.save_settings(&s).unwrap();
        assert_eq!(store.settings().interval_secs, 10);
        store.wipe().unwrap();
        assert!(store.search("stripe", 10).unwrap().is_empty());
        assert!(store.fts_integrity_ok());
    }
}

#[cfg(test)]
mod live {
    //! RAT_DB=/path/to/copy.db cargo test live_reclean -- --ignored --nocapture
    //! Re-runs cleanup on a COPY of a real database and prints the savings.
    #[test]
    #[ignore]
    fn live_reclean() {
        let path = std::env::var("RAT_DB").expect("RAT_DB");
        let store = super::Store::open_plain(std::path::Path::new(&path)).unwrap();
        let before = store.stats(0).unwrap();
        let s = store.reclean_all().unwrap();
        let after = store.stats(0).unwrap();
        println!(
            "snapshots {} → {} (removed {}), chars {} → {} ({:.0}% smaller)",
            before.snapshots, after.snapshots, s.removed_empty, s.raw_chars, s.clean_chars,
            100.0 * (1.0 - s.clean_chars as f64 / s.raw_chars.max(1) as f64)
        );
    }

    #[test]
    fn old_style_memory_index_is_recreated() {
        use super::{NewMemory, Store};
        let dir = std::env::temp_dir().join(format!("rat-fts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.db");
        {
            let store = Store::open_plain(&path).unwrap();
            let id = store.start_activity("Safari", None, "Budget", Some("https://x.test/b"), 1).unwrap();
            let m = NewMemory { kind: "reading".into(), title: "Vatsalya budget".into(), summary: "Budget is 35 lakh".into(), organizations: vec!["Vatsalya".into()], numbers: vec!["35 lakh".into()], keep: true, confidence: 0.9, model: "t".into(), ..Default::default() };
            store.insert_memory(id, &m, 2).unwrap();
            // Regress the index to the pre-view definition.
            store.conn.execute_batch(
                "DROP TABLE memories_fts;
                 CREATE VIRTUAL TABLE memories_fts USING fts5(title, summary, entities, content='memories', content_rowid='id', tokenize='porter unicode61');",
            ).unwrap();
            assert!(store.conn.execute_batch("INSERT INTO memories_fts(memories_fts) VALUES ('rebuild')").is_err());
        }
        let store = Store::open_plain(&path).unwrap();
        let sql: String = store.conn.query_row("SELECT sql FROM sqlite_master WHERE name = 'memories_fts'", [], |r| r.get(0)).unwrap();
        assert!(sql.contains("memories_content"));
        let hits = store.search_memories("Vatsalya", None, true, 10).unwrap();
        assert_eq!(hits.len(), 1, "search works again after the index is recreated");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn facts_conflict_and_user_corrections_survive_reprocessing() {
        use super::{NewFact, NewMemory, Store};
        let store = Store::open_in_memory().unwrap();
        let a1 = store.start_activity("Safari", None, "Budget v1", Some("https://x.test/v1"), 1_000).unwrap();
        let m1 = NewMemory { kind: "document".into(), title: "Budget v1".into(), summary: "s".into(), keep: true, confidence: 0.9, model: "t".into(),
            facts: vec![NewFact { subject: "Vatsalya proposal".into(), attribute: "budget".into(), value: "₹35 lakh".into(), owner: "unknown".into(), stance: "stated".into() }], ..Default::default() };
        let id1 = store.insert_memory(a1, &m1, 2_000).unwrap();
        let a2 = store.start_activity("Safari", None, "Budget v2", Some("https://x.test/v2"), 5_000).unwrap();
        let m2 = NewMemory { kind: "document".into(), title: "Budget v2".into(), summary: "s".into(), keep: true, confidence: 0.9, model: "t".into(),
            facts: vec![NewFact { subject: "vatsalya  Proposal".into(), attribute: "Budget".into(), value: "₹37 lakh".into(), owner: "unknown".into(), stance: "stated".into() }], ..Default::default() };
        let id2 = store.insert_memory(a2, &m2, 6_000).unwrap();

        // Same key, different values: each sees the other as a conflict.
        let f1 = store.facts_for(&[id1]).unwrap();
        assert_eq!(f1.len(), 1);
        assert_eq!(f1[0].conflicts.len(), 1);
        assert_eq!(f1[0].conflicts[0].value, "₹37 lakh");
        assert_eq!(store.conflicting_facts(10).unwrap().len(), 2);
        assert_eq!(store.search_facts("vatsalya budget", 5).unwrap().len(), 2);

        // The user corrects the first: model row retracted, user row active.
        store.correct_fact(f1[0].id, "₹36 lakh", 7_000).unwrap();
        let f1 = store.facts_for(&[id1]).unwrap();
        assert_eq!(f1.len(), 1);
        assert_eq!((f1[0].origin.as_str(), f1[0].value.as_str()), ("user", "₹36 lakh"));

        // Reprocessing the activity keeps the user's fact and ignores the
        // model's same-key fact.
        let id1b = store.insert_memory(a1, &m1, 8_000).unwrap();
        let f1 = store.facts_for(&[id1b]).unwrap();
        assert_eq!(f1.len(), 1);
        assert_eq!((f1[0].origin.as_str(), f1[0].value.as_str()), ("user", "₹36 lakh"));

        // An edited memory is never replaced by reprocessing.
        store.update_memory(id2, "Budget v2 (final)".into(), "Final figure".into(), &[], &[], &["Vatsalya".into()], &[], 9_000).unwrap();
        let same = store.insert_memory(a2, &m2, 10_000).unwrap();
        assert_eq!(same, id2);
        let card = store.memory_by_id(id2).unwrap().unwrap();
        assert_eq!(card.title, "Budget v2 (final)");
        assert!(card.edited_at.is_some());
        assert_eq!(store.search_memories("final", None, true, 5).unwrap().len(), 1, "edits reach the search index");

        // Pinned memories rank first.
        store.set_pinned(&[id1b], true).unwrap();
        let hits = store.search_memories("budget", None, true, 5).unwrap();
        assert_eq!(hits[0].id, id1b);
        assert!(hits[0].pinned);
    }

    #[test]
    fn dropped_memories_do_not_crowd_out_kept_ones() {
        use super::{NewMemory, Store};
        let store = Store::open_in_memory().unwrap();
        // Twenty short "not worth keeping" file-name memories score high on bm25.
        for i in 0..20 {
            let a = store.start_activity("Finder", None, "Vatsalya_DPR.docx", None, i * 1000).unwrap();
            let m = NewMemory { kind: "other".into(), title: "Vatsalya_DPR.docx".into(), summary: String::new(), keep: false, confidence: 0.9, model: "t".into(), ..Default::default() };
            store.insert_memory(a, &m, i * 1000 + 1).unwrap();
        }
        let a = store.start_activity("Safari", None, "Proposal", Some("https://x.test/p"), 99_000).unwrap();
        let m = NewMemory { kind: "document".into(), title: "Vatsalya Scheme communication proposal with budget".into(), summary: "A long proposal about the Vatsalya scheme".into(), keep: true, confidence: 0.9, model: "t".into(), ..Default::default() };
        let kept = store.insert_memory(a, &m, 99_001).unwrap();
        let hits = store.search_memories("Vatsalya", None, true, 2).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, kept);
        assert!(store.search_memories("Vatsalya", None, false, 50).unwrap().len() > 1, "everything shows when not filtering");
    }

    #[test]
    fn written_dates_parse() {
        use super::parse_written_date;
        let today = crate::engine::local_midnight(2026, 9, 19).unwrap();
        let oct14 = crate::engine::local_midnight(2026, 10, 14).unwrap();
        for s in ["14 October", "14 Oct", "October 14", "Oct 14, 2026", "14/10/2026", "2026-10-14", "14-10-2026", "14th October 2026"] {
            assert_eq!(parse_written_date(s, today), Some(oct14), "{s}");
        }
        assert_eq!(parse_written_date("3 January", today), crate::engine::local_midnight(2027, 1, 3), "no year = next occurrence");
        assert_eq!(parse_written_date("₹35 lakh", today), None);
        assert_eq!(parse_written_date("Q3", today), None);
    }

    #[test]
    fn forgetting_a_name_removes_it_everywhere() {
        use super::{NewFact, NewMemory, Store};
        let store = Store::open_in_memory().unwrap();
        let a = store.start_activity("Safari", None, "Meeting with Sarah Khan", Some("https://x.test/m"), 1_000).unwrap();
        store.extend_activity(a, 60_000).unwrap();
        store.add_snapshot(a, "Sarah Khan said the tender closes on 14 October. Call SARAH tomorrow.", 2_000).unwrap();
        let m = NewMemory { kind: "chat".into(), title: "Call with Sarah Khan".into(), summary: "Sarah Khan confirmed the tender date.".into(), people: vec!["Sarah Khan".into(), "Ravi".into()], keep: true, confidence: 0.9, model: "t".into(),
            tasks: vec!["Call Sarah Khan tomorrow".into()], facts: vec![NewFact { subject: "tender".into(), attribute: "closes".into(), value: "14 October".into(), owner: "unknown".into(), stance: "stated".into() }, NewFact { subject: "Sarah Khan".into(), attribute: "role".into(), value: "buyer".into(), owner: "unknown".into(), stance: "stated".into() }], ..Default::default() };
        let id = store.insert_memory(a, &m, 3_000).unwrap();
        store.save_recap("2026-09-18", "You spoke with Sarah Khan about the tender.", "[]", 4_000).unwrap();
        let r = store.forget_term("sarah khan").unwrap();
        assert_eq!((r.entities, r.memories, r.facts, r.tasks, r.snapshots, r.recaps), (1, 1, 1, 1, 1, 1));
        let card = store.memory_by_id(id).unwrap().unwrap();
        assert_eq!(card.title, "Call with [forgotten]");
        assert_eq!(card.people, vec!["Ravi".to_string()]);
        assert!(store.search_memories("Sarah", None, true, 5).unwrap().is_empty(), "gone from the memory index");
        assert!(store.search("Khan", 5).unwrap().is_empty(), "gone from raw text search");
        assert!(!store.search("SARAH", 5).unwrap().is_empty(), "a different token is left alone: forgetting is term-exact");
        assert_eq!(store.facts_for(&[id]).unwrap().len(), 1, "unrelated fact kept");
        assert!(store.list_entities(Some("person"), Some("Sarah"), 5).unwrap().is_empty());
        let text = store.activity_text(a, 500).unwrap();
        assert!(!text.to_lowercase().contains("sarah khan"), "{text}");
    }

    #[test]
    fn raw_text_is_pruned_but_meetings_and_notes_stay() {
        use super::{NewMemory, Store};
        let store = Store::open_in_memory().unwrap();
        let old = store.start_activity("Safari", None, "Old page", None, 1_000).unwrap();
        store.add_snapshot(old, "old raw text that can go", 1_500).unwrap();
        store.insert_memory(old, &NewMemory { kind: "article".into(), title: "Old".into(), summary: "s".into(), keep: true, confidence: 0.9, model: "t".into(), ..Default::default() }, 2_000).unwrap();
        let meeting = store.start_activity("Meeting", None, "Standup", None, 1_000).unwrap();
        store.add_snapshot(meeting, "You: transcript stays forever", 1_500).unwrap();
        store.insert_memory(meeting, &NewMemory { kind: "meeting".into(), title: "Standup".into(), summary: "s".into(), keep: true, confidence: 0.9, model: "t".into(), ..Default::default() }, 2_000).unwrap();
        let fresh = store.start_activity("Safari", None, "New page", None, 900_000).unwrap();
        store.add_snapshot(fresh, "fresh raw text stays for now", 900_500).unwrap();
        let n = store.prune_raw(500_000, &["Meeting".into(), "Note".into()]).unwrap();
        assert_eq!(n, 1);
        assert_eq!(store.activity_text(old, 100).unwrap(), "");
        assert!(store.activity_text(meeting, 100).unwrap().contains("transcript"));
        assert!(store.activity_text(fresh, 100).unwrap().contains("fresh"));
        assert_eq!(store.memory_by_id(1).unwrap().map(|c| c.title), Some("Old".into()), "the memory survives");
    }

    #[test]
    fn quantised_index_ranks_like_exact_dot_products() {
        use super::QuantIndex;
        let mut idx = QuantIndex::new(4);
        let vecs: Vec<[f32; 4]> = vec![[1.0, 0.0, 0.0, 0.0], [0.7, 0.7, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.1, 0.1, 0.98, 0.0]];
        for (i, v) in vecs.iter().enumerate() {
            idx.push(i as i64 + 1, v);
        }
        let top = idx.search(&[0.9, 0.1, 0.0, 0.0], 2);
        assert_eq!(top.iter().map(|x| x.0).collect::<Vec<_>>(), vec![1, 2]);
        assert!((top[0].1 - 0.9).abs() < 0.02, "quantisation error stays small: {}", top[0].1);
        assert_eq!(idx.search(&[0.0, 0.0, 1.0, 0.0], 1)[0].0, 4);
        assert!(idx.search(&[1.0, 0.0], 3).is_empty(), "wrong dimension yields nothing");
    }
}
