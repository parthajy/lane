//! Capture loop: poll the frontmost window, turn a stream of observations
//! into activities + distinct text snapshots, write them to the store.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos as platform;

#[cfg(not(target_os = "macos"))]
pub mod platform {
    use super::Observation;
    pub fn is_trusted(_prompt: bool) -> bool { false }
    pub fn accessibility_works() -> bool { false }
    pub fn idle_seconds() -> f64 { 0.0 }
    pub fn observe(_read_browser_text: bool, _with_text: bool) -> Option<Observation> { None }
    pub fn observe_front_other(_read_browser_text: bool, _with_text: bool) -> Option<Observation> { None }
    pub fn focused_text() -> Option<String> { None }
    pub fn press_paste() {}
}

use crate::privacy;
use crate::store::Store;
use crate::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// What the platform layer saw in front of the user at one instant.
#[derive(Debug, Clone, Default)]
pub struct Observation {
    pub app_name: String,
    pub bundle_id: Option<String>,
    pub app_path: Option<String>,
    pub window_title: String,
    pub url: Option<String>,
    /// Visible text of the focused window, if readable.
    pub text: Option<String>,
}

/// Minimum time between stored snapshots of the same activity.
const SNAPSHOT_MIN_GAP_MS: i64 = 30_000;
/// Activities shorter than this with no text are alt-tab noise.
const TRIVIAL_ACTIVITY_MS: i64 = 3_000;
/// Coming back to the same window within this long resumes the earlier
/// activity instead of starting a new one (quick replies, alt-tab checks).
const RESUME_WINDOW_MS: i64 = 90_000;
const RECENT_MAX: usize = 8;

#[derive(Debug, PartialEq)]
pub enum Change {
    None,
    Started(i64),
    Closed,
    Switched(i64),
}

#[derive(Debug, Clone, PartialEq)]
struct Key {
    app: String,
    title: String,
    url: Option<String>,
}

impl Key {
    fn of(obs: &Observation) -> Self {
        Self {
            app: obs.app_path.clone().unwrap_or_else(|| obs.app_name.clone()),
            title: normalize_title(&obs.window_title),
            url: obs.url.clone(),
        }
    }

    /// Same window. A missing URL is "unknown", not "different": the
    /// address bar popup or a page mid-load briefly reports none.
    fn matches(&self, other: &Key) -> bool {
        self.app == other.app
            && self.title == other.title
            && (self.url == other.url || self.url.is_none() || other.url.is_none())
    }
}

#[derive(Debug, Clone)]
struct Current {
    id: i64,
    key: Key,
    last_seen: i64,
    last_snapshot_at: Option<i64>,
    had_text: bool,
}

pub struct Sessionizer {
    current: Option<Current>,
    pub redact_contacts: bool,
    /// Keep a picture of the window with each new snapshot (opt-in).
    pub screenshots: bool,
    pub data_dir: std::path::PathBuf,
    /// Recently closed activities, newest last, for resumption.
    recent: Vec<Current>,
    /// If polls stop for longer than this (sleep, pause), the activity ends.
    max_gap_ms: i64,
}

/// Strip volatile title prefixes ("(3) Slack", "• Inbox") so unread
/// counters don't split one activity into many.
/// "Vatsalya_DPR.pdf", "Budget.xlsx - WPS Office", "report.docx — Pages" →
/// the file name, when the title carries one with a known document type.
pub fn document_name(title: &str) -> Option<String> {
    let t = title.trim();
    for sep in [" - ", " — ", " – ", " | "] {
        if let Some((head, _)) = t.split_once(sep) {
            if let Some(n) = document_name(head) {
                return Some(n);
            }
        }
    }
    let (_, ext) = t.rsplit_once('.')?;
    let ext = ext.trim().to_lowercase();
    if ext.chars().count() > 5 || !crate::files::is_indexable(ext.as_str()) || crate::files::IMAGE_EXTS.contains(&ext.as_str()) {
        return None;
    }
    Some(t.to_string())
}

pub fn normalize_title(title: &str) -> String {
    let mut t = title.trim();
    loop {
        let before = t;
        if t.starts_with('(') {
            if let Some(end) = t.find(')') {
                if t[1..end].chars().all(|c| c.is_ascii_digit() || c == '+') && end > 1 {
                    t = t[end + 1..].trim_start();
                }
            }
        }
        t = t.trim_start_matches(|c| c == '•' || c == '●').trim_start();
        if t == before {
            break;
        }
    }
    t.to_string()
}

impl Sessionizer {
    pub fn new(interval_secs: u64) -> Self {
        Self { current: None, redact_contacts: true, screenshots: false, data_dir: std::path::PathBuf::new(), recent: Vec::new(), max_gap_ms: (interval_secs as i64 * 3 * 1000).max(15_000) }
    }

    pub fn set_interval(&mut self, interval_secs: u64) {
        self.max_gap_ms = (interval_secs as i64 * 3 * 1000).max(15_000);
    }

    pub fn current_id(&self) -> Option<i64> {
        self.current.as_ref().map(|c| c.id)
    }

    /// Should the caller do the expensive text read for this observation?
    /// Yes for a window we are not already tracking, and whenever the
    /// current activity is due for a snapshot.
    pub fn wants_text(&self, obs: &Observation, now: i64) -> bool {
        let key = Key::of(obs);
        match &self.current {
            Some(c) if c.key.matches(&key) && now - c.last_seen <= self.max_gap_ms => {
                c.last_snapshot_at.map_or(true, |t| now - t >= SNAPSHOT_MIN_GAP_MS)
            }
            _ => true,
        }
    }

    pub fn observe(&mut self, store: &Store, obs: Option<Observation>, now: i64) -> rusqlite::Result<Change> {
        let Some(obs) = obs else {
            return Ok(if self.close(store)? { Change::Closed } else { Change::None });
        };

        let key = Key::of(&obs);
        let continues = matches!(&self.current, Some(c) if c.key.matches(&key) && now - c.last_seen <= self.max_gap_ms);
        let mut change = Change::None;
        if continues {
            let cur = self.current.as_mut().expect("checked");
            cur.last_seen = now;
            if cur.key.url.is_none() {
                cur.key.url = key.url.clone();
            }
            store.extend_activity(cur.id, now)?;
        } else {
            let had_current = self.close(store)?;
            let resumed = self
                .recent
                .iter()
                .rposition(|r| r.key.matches(&key) && now - r.last_seen <= RESUME_WINDOW_MS)
                .map(|i| self.recent.remove(i));
            let cur = match resumed {
                Some(mut r) => {
                    r.last_seen = now;
                    store.extend_activity(r.id, now)?;
                    r
                }
                None => {
                    let id = store.start_activity(&obs.app_name, obs.app_path.as_deref(), &key.title, key.url.as_deref(), now)?;
                    Current { id, key, last_seen: now, last_snapshot_at: None, had_text: false }
                }
            };
            change = if had_current { Change::Switched(cur.id) } else { Change::Started(cur.id) };
            self.current = Some(cur);
        }

        if let Some(text) = obs.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            let cur = self.current.as_mut().expect("set above");
            if cur.last_snapshot_at.map_or(true, |t| now - t >= SNAPSHOT_MIN_GAP_MS) {
                let text = privacy::redact_all(text, self.redact_contacts);
                if let Some(snapshot_id) = store.add_snapshot(cur.id, &text, now)? {
                    cur.had_text = true;
                    if self.screenshots {
                        if let Some(path) = crate::shots::take(&self.data_dir, cur.id, now) {
                            let _ = store.set_snapshot_image(snapshot_id, &path);
                        }
                    }
                }
                // Even a duplicate counts as "checked": wait a full gap
                // before reading the tree again.
                cur.last_snapshot_at = Some(now);
            }
        }
        Ok(change)
    }

    /// End the current activity. Returns true if there was one.
    pub fn close(&mut self, store: &Store) -> rusqlite::Result<bool> {
        let Some(cur) = self.current.take() else { return Ok(false) };
        let discarded = store.discard_if_trivial(cur.id, TRIVIAL_ACTIVITY_MS)?;
        if !discarded {
            self.recent.push(cur);
            if self.recent.len() > RECENT_MAX {
                self.recent.remove(0);
            }
        }
        Ok(true)
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Runs for the life of the app on its own thread.
pub fn spawn(app: AppHandle, state: Arc<AppState>) {
    std::thread::Builder::new()
        .name("capture".into())
        .spawn(move || {
            let initial = crate::lock(&state.settings).interval_secs;
            let mut sessions = Sessionizer::new(initial);
            let mut last_heartbeat = 0i64;
            loop {
                let settings = crate::lock(&state.settings).clone();
                sessions.set_interval(settings.interval_secs);
                sessions.redact_contacts = settings.redact_contacts;
                sessions.screenshots = settings.screenshots_enabled;
                if sessions.data_dir.as_os_str().is_empty() {
                    sessions.data_dir = state.db_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                }
                let now = now_ms();

                let paused = state.paused.load(Ordering::Relaxed);
                let idle = platform::idle_seconds() >= (settings.idle_minutes * 60) as f64;
                let trusted = platform::is_trusted(false);
                if trusted {
                    state.stale_grant.store(false, Ordering::Relaxed);
                    let build = crate::permissions::build_id();
                    if settings.trusted_build != build {
                        let mut s = crate::lock(&state.settings);
                        s.trusted_build = build;
                        let _ = crate::lock(&state.store).save_settings(&s);
                    }
                }

                let mut excluded = false;
                let observation = if paused || idle {
                    None
                } else {
                    // Cheap look first (app, title, URL); walk the tree for
                    // text only when this activity is due for a snapshot.
                    platform::observe(settings.read_browser_text, false)
                        .filter(|o| {
                            excluded = privacy::is_excluded(&settings, &o.app_name, o.bundle_id.as_deref(), o.url.as_deref());
                            !excluded
                        })
                        .map(|light| {
                            if sessions.wants_text(&light, now) {
                                platform::observe(settings.read_browser_text, true)
                                    .filter(|full| Key::of(full).matches(&Key::of(&light)))
                                    .unwrap_or(light)
                            } else {
                                light
                            }
                        })
                };
                let seen = observation.as_ref().map(|o| (o.app_name.clone(), normalize_title(&o.window_title)));

                let change = {
                    let store = crate::lock(&state.store);
                    // A document app that shows a file name but no text (WPS,
                    // Preview, Word): the indexed document stands in.
                    let observation = observation.map(|mut o| {
                        let thin = o.text.as_ref().map_or(true, |t| t.chars().count() < 300);
                        if thin && sessions.wants_text(&o, now) {
                            if let Some(name) = document_name(&o.window_title) {
                                if let Ok(Some((path, text))) = store.file_text_by_name(&name, 12_000) {
                                    if !text.trim().is_empty() {
                                        o.text = Some(format!("Document: {name}\n{text}"));
                                        if o.url.is_none() {
                                            o.url = Some(format!("file://{path}"));
                                        }
                                    }
                                }
                            }
                        }
                        o
                    });
                    if let Some(o) = observation.as_ref() {
                        *crate::lock(&state.last_seen) = Some(o.clone());
                    }
                    sessions.observe(&store, observation, now).unwrap_or_else(|e| {
                        log::error!("capture write failed: {e}");
                        Change::None
                    })
                };

                {
                    let mut status = crate::lock(&state.status);
                    status.trusted = trusted;
                    status.idle = idle;
                    status.excluded = excluded;
                    status.current_app = seen.as_ref().map(|s| s.0.clone());
                    status.current_title = seen.map(|s| s.1);
                    if sessions.current_id().is_some() {
                        status.last_capture_at = Some(now);
                    }
                }
                if change != Change::None {
                    let _ = app.emit("activity-changed", ());
                }
                // One line every 10 minutes proves the loop is alive in the log.
                if now - last_heartbeat > 600_000 {
                    last_heartbeat = now;
                    log::info!("capture: alive, trusted={trusted}, idle={idle}, paused={paused}");
                }

                if state.blocked.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_secs(20));
                    continue;
                }
                std::thread::sleep(Duration::from_secs(settings.interval_secs.clamp(2, 60)));
            }
        })
        .expect("spawn capture thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(app: &str, title: &str, text: Option<&str>) -> Option<Observation> {
        Some(Observation {
            app_name: app.into(),
            app_path: Some(format!("/Applications/{app}.app")),
            window_title: title.into(),
            text: text.map(Into::into),
            ..Default::default()
        })
    }

    fn with_url(o: Option<Observation>, url: Option<&str>) -> Option<Observation> {
        o.map(|mut o| {
            o.url = url.map(Into::into);
            o
        })
    }

    fn count(store: &Store, table: &str) -> i64 {
        store.stats(0).map(|s| if table == "activities" { s.activities } else { s.snapshots }).unwrap()
    }

    #[test]
    fn same_window_extends_one_activity() {
        let store = Store::open_in_memory().unwrap();
        let mut s = Sessionizer::new(5);
        assert!(matches!(s.observe(&store, obs("Notes", "Plan", Some("alpha text")), 0).unwrap(), Change::Started(_)));
        for t in [5_000, 10_000, 15_000] {
            assert_eq!(s.observe(&store, obs("Notes", "Plan", Some("alpha text")), t).unwrap(), Change::None);
        }
        assert_eq!(count(&store, "activities"), 1);
        assert_eq!(count(&store, "snapshots"), 1, "identical text is stored once");
        let a = &store.list_activities(None, 10).unwrap()[0];
        assert_eq!(a.ended_at - a.started_at, 15_000);
    }

    #[test]
    fn switching_windows_starts_new_activity() {
        let store = Store::open_in_memory().unwrap();
        let mut s = Sessionizer::new(5);
        s.observe(&store, obs("Notes", "Plan", Some("plan text")), 0).unwrap();
        s.observe(&store, obs("Notes", "Plan", Some("plan text")), 10_000).unwrap();
        assert!(matches!(s.observe(&store, obs("Safari", "News", Some("news text")), 15_000).unwrap(), Change::Switched(_)));
        assert_eq!(count(&store, "activities"), 2);
    }

    #[test]
    fn unread_counters_do_not_split_activities() {
        assert_eq!(normalize_title("(3) Slack | general"), "Slack | general");
        assert_eq!(normalize_title("• (12) Inbox"), "Inbox");
        assert_eq!(normalize_title("(draft) notes"), "(draft) notes");
        let store = Store::open_in_memory().unwrap();
        let mut s = Sessionizer::new(5);
        s.observe(&store, obs("Slack", "(2) Slack | general", None), 0).unwrap();
        assert_eq!(s.observe(&store, obs("Slack", "(3) Slack | general", None), 5_000).unwrap(), Change::None);
    }

    #[test]
    fn text_is_wanted_at_start_and_every_gap() {
        let store = Store::open_in_memory().unwrap();
        let mut s = Sessionizer::new(5);
        let o = obs("Notes", "Plan", None).unwrap();
        assert!(s.wants_text(&o, 0), "new window");
        s.observe(&store, obs("Notes", "Plan", Some("version one")), 0).unwrap();
        assert!(!s.wants_text(&o, 5_000), "inside the gap");
        assert!(s.wants_text(&o, 30_000), "gap passed");
        // Text offered inside the gap is ignored, not stored.
        s.observe(&store, obs("Notes", "Plan", Some("version two")), 10_000).unwrap();
        assert_eq!(count(&store, "snapshots"), 1);
        s.observe(&store, obs("Notes", "Plan", Some("version three")), 30_000).unwrap();
        assert_eq!(count(&store, "snapshots"), 2);
    }

    #[test]
    fn page_not_ready_is_retried_next_poll() {
        let store = Store::open_in_memory().unwrap();
        let mut s = Sessionizer::new(5);
        let o = with_url(obs("Google Chrome", "Claude", None), Some("https://claude.ai/chat/1")).unwrap();
        s.observe(&store, Some(o.clone()), 0).unwrap();
        assert!(s.wants_text(&o, 5_000), "no snapshot yet, keep asking");
    }

    #[test]
    fn quick_return_resumes_the_same_activity() {
        let store = Store::open_in_memory().unwrap();
        let mut s = Sessionizer::new(5);
        s.observe(&store, obs("Notes", "Plan", Some("plan text")), 0).unwrap();
        s.observe(&store, obs("Notes", "Plan", None), 20_000).unwrap();
        s.observe(&store, obs("Slack", "general", Some("hello there")), 25_000).unwrap();
        // Back within 90s: same activity continues.
        assert!(matches!(s.observe(&store, obs("Notes", "Plan", None), 40_000).unwrap(), Change::Switched(1)));
        assert_eq!(count(&store, "activities"), 2);
        let plan = store.get_activity(1).unwrap().unwrap().activity;
        assert_eq!(plan.ended_at, 40_000);
        // Back after a long time: a new activity.
        s.observe(&store, obs("Slack", "general", None), 45_000).unwrap();
        assert!(matches!(s.observe(&store, obs("Notes", "Plan", None), 200_000).unwrap(), Change::Switched(3)));
    }

    #[test]
    fn missing_url_does_not_split_a_page_visit() {
        let store = Store::open_in_memory().unwrap();
        let mut s = Sessionizer::new(5);
        let page = Some("https://claude.ai/chat/1");
        s.observe(&store, with_url(obs("Google Chrome", "Claude", Some("alpha text")), page), 0).unwrap();
        // Address bar popup: same title, URL unknown for one poll.
        assert_eq!(s.observe(&store, with_url(obs("Google Chrome", "Claude", None), None), 5_000).unwrap(), Change::None);
        assert_eq!(s.observe(&store, with_url(obs("Google Chrome", "Claude", None), page), 10_000).unwrap(), Change::None);
        // A different page with the same title is a new activity.
        assert!(matches!(
            s.observe(&store, with_url(obs("Google Chrome", "Claude", None), Some("https://claude.ai/chat/2")), 15_000).unwrap(),
            Change::Switched(_)
        ));
    }

    #[test]
    fn gap_in_polling_ends_activity() {
        let store = Store::open_in_memory().unwrap();
        let mut s = Sessionizer::new(5);
        s.observe(&store, obs("Notes", "Plan", Some("some text")), 0).unwrap();
        // Mac slept for 10 minutes: same window, but a new activity.
        assert!(matches!(s.observe(&store, obs("Notes", "Plan", Some("some text")), 600_000).unwrap(), Change::Switched(_)));
        assert_eq!(count(&store, "activities"), 2);
    }

    #[test]
    fn alt_tab_flicker_is_discarded() {
        let store = Store::open_in_memory().unwrap();
        let mut s = Sessionizer::new(2);
        s.observe(&store, obs("Finder", "", None), 0).unwrap();
        s.observe(&store, obs("Notes", "Plan", Some("some text")), 2_000).unwrap();
        assert_eq!(count(&store, "activities"), 1, "textless 2s Finder blip dropped");
        // …and it is not resumable either.
        s.observe(&store, obs("Finder", "", None), 4_000).unwrap();
        assert_eq!(count(&store, "activities"), 2);
    }

    #[test]
    fn card_numbers_and_contacts_never_reach_disk() {
        let store = Store::open_in_memory().unwrap();
        let mut s = Sessionizer::new(5);
        s.observe(&store, obs("Safari", "Checkout", Some("Pay with 4242 4242 4242 4242, call 7002808244")), 0).unwrap();
        assert!(store.search("4242", 10).unwrap().is_empty());
        assert!(store.search("7002808244", 10).unwrap().is_empty());
        assert_eq!(store.search("redacted", 10).unwrap().len(), 1);
        assert_eq!(store.search("phone", 10).unwrap().len(), 1);
    }

    #[test]
    fn document_names_are_found_in_titles() {
        assert_eq!(document_name("Vatsalya_DPR.pdf").as_deref(), Some("Vatsalya_DPR.pdf"));
        assert_eq!(document_name("Budget 2026.xlsx - WPS Office").as_deref(), Some("Budget 2026.xlsx"));
        assert_eq!(document_name("report.docx — Pages").as_deref(), Some("report.docx"));
        assert_eq!(document_name("Inbox (12) - Gmail"), None);
        assert_eq!(document_name("photo.png"), None, "images are not documents to read");
        assert_eq!(document_name("v2.1 release notes"), None);
    }
}
