//! Dictation: press the shortcut, speak, press again; the words land in
//! whatever field has focus. Mic only, transcribed on this Mac by the same
//! speech engine meetings use, inserted through the clipboard and ⌘V.

use crate::AppState;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

static SESSION: Mutex<Option<PathBuf>> = Mutex::new(None);

fn announce(app: &AppHandle, active: bool) {
    let _ = app.emit("dictation-state", serde_json::json!({"active": active}));
}

pub fn is_active() -> bool {
    crate::lock(&SESSION).is_some()
}

/// Read by the notch and the meetings page: a dictation is not a meeting.
pub fn active() -> bool {
    is_active()
}

/// Start or stop. Returns what happened, for the notch and the log.
pub fn toggle(app: &AppHandle, state: &AppState) -> Result<String, String> {
    if is_active() {
        return finish(app, state);
    }
    if crate::lock(&crate::meetings::RECORDER).is_some() {
        return Err("A meeting is being recorded; stop it first".into());
    }
    let (ready, why) = crate::engine::meeting_readiness(state);
    if !ready {
        return Err(why);
    }
    let helper = crate::meetings::helper_path(state.resource_dir.as_deref()).ok_or("recorder missing")?;
    let dir = std::env::temp_dir().join(format!("lane-dictation-{}", crate::capture::now_ms()));
    crate::meetings::start(&helper, &dir, 0, crate::capture::now_ms(), state.recording.clone(), true)?;
    *crate::lock(&SESSION) = Some(dir);
    announce(app, true);
    crate::engine::notch(app, "dictation", "Listening", vec!["Speak. Press ⌥⇧Space again (or Insert) to put the words where your cursor is.".into()]);
    log::info!("dictation: started");
    Ok("listening".into())
}

fn finish(app: &AppHandle, state: &AppState) -> Result<String, String> {
    let dir = crate::lock(&SESSION).take().ok_or("not dictating")?;
    let _ = crate::meetings::stop();
    {
        let mut r = crate::lock(&state.recording);
        r.recording = false;
        r.live_text.clear();
    }
    let whisper = crate::meetings::whisper_path(state.resource_dir.as_deref()).ok_or("speech engine missing")?;
    let model = state.db_path.with_file_name("models").join(crate::meetings::whisper_model().file);
    let lang = crate::lock(&state.settings).meeting_language.clone();
    let segs = crate::meetings::transcribe(&whisper, &model, &dir.join("mic.wav"), if lang.is_empty() { "auto" } else { &lang })?;
    let _ = std::fs::remove_dir_all(&dir);
    announce(app, false);
    let text: String = segs.iter().map(|s| s.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        crate::engine::notch(app, "help", "Dictation", vec!["Nothing was heard.".into()]);
        return Ok("empty".into());
    }
    insert_text(&text)?;
    // Dictated words are worth remembering too: a voice note, made into a memory.
    if let Err(e) = crate::lock(&state.store).create_note(&text, crate::capture::now_ms(), "Voice note") {
        log::warn!("dictation: could not keep the note: {e}");
    } else {
        state.engine_wake.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    crate::engine::notch(app, "help", "Inserted and remembered", vec![text.chars().take(160).collect()]);
    let _ = app.emit("dictation-done", serde_json::json!({"text": text}));
    log::info!("dictation: inserted {} chars", text.chars().count());
    Ok(text)
}

/// Put the text where the cursor is: clipboard, then ⌘V, then the old
/// clipboard back. Needs Accessibility, which capture already has.
pub fn insert_text(text: &str) -> Result<(), String> {
    let mut board = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let previous = board.get_text().ok();
    board.set_text(text.to_string()).map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(60));
    crate::capture::platform::press_paste();
    std::thread::sleep(std::time::Duration::from_millis(250));
    if let Some(p) = previous {
        let _ = board.set_text(p);
    }
    Ok(())
}
