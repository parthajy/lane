//! Dictation: press the shortcut and speak. The words appear as you say
//! them, and when you stop talking they are put where the cursor is. Press
//! the shortcut again to finish early.
//!
//! Mic only, transcribed on this Mac by the same speech engine meetings use,
//! inserted through the clipboard and ⌘V.

use crate::AppState;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

static SESSION: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Set while the last pass is running. The session folder is taken at the
/// top of `finish`, but the words are not in yet and the microphone is only
/// just closing; without this the shortcut pressed a moment later would read
/// "not dictating" and start a second recording on top of the first.
static FINISHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Holds `FINISHING` up for as long as it is alive, however `finish` returns.
struct Finishing;

impl Finishing {
    fn start() -> Self {
        FINISHING.store(true, std::sync::atomic::Ordering::SeqCst);
        Finishing
    }
}

impl Drop for Finishing {
    fn drop(&mut self) {
        FINISHING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

fn announce(app: &AppHandle, active: bool) {
    let _ = app.emit("dictation-state", serde_json::json!({"active": active}));
}

pub fn is_active() -> bool {
    crate::lock(&SESSION).is_some() || FINISHING.load(std::sync::atomic::Ordering::SeqCst)
}

/// Read by the notch and the meetings page: a dictation is not a meeting.
pub fn active() -> bool {
    is_active()
}

/// Start or stop. Returns what happened, for the notch and the log.
// ── Watching the words arrive ────────────────────────────────────────────
//
// Every pass transcribes the whole utterance rather than only the newest
// slice. Whisper costs about 0.8s to start and a tenth of a second per
// second of audio, so a ten second dictation re-reads in under two, and
// doing it whole means no seam where one chunk met the next: the text simply
// improves as more of the sentence arrives.
//
// When the microphone goes quiet the dictation finishes itself, so the
// normal way to use it is to press the key once, speak, and stop.

/// The best transcript so far, and how many bytes of audio it covers.
static LIVE: Mutex<(String, u64)> = Mutex::new((String::new(), 0));
/// How long to wait between passes.
const PACE: Duration = Duration::from_millis(900);
/// Quiet for this long, with something said, and it puts the words in.
///
/// Was under two seconds, which is shorter than thinking about the next
/// sentence. People stopped to consider a word and Lane decided they had
/// finished. Long enough now to gather a thought; anyone who wants it in
/// sooner presses the shortcut, which is instant.
const HUSH: i64 = 4_500;
/// Below this there is not enough sound to be worth reading.
const MIN_AUDIO: u64 = 16_000 * 2 / 2; // half a second at 16 kHz, int16
/// Bytes of 16 kHz mono int16 audio in a millisecond.
const PER_MS: u64 = 32;
/// Re-reading the whole utterance costs about a tenth of a second per second
/// of audio, so a long dictation would slow to a crawl. Past this much
/// audio the settled part is put aside and only the end is re-read.
const WINDOW: u64 = PER_MS * 1_000 * 20;
/// How much of the window to leave live when the rest is put aside: enough
/// that the last words can still change as the sentence finishes.
const KEEP_MS: i64 = 4_000;
/// The level meter in the recorder is one way to tell that someone has
/// stopped talking, and on a quiet microphone it never trips at all. This is
/// the other way, measured from the samples here: this much audio with no
/// sound in it ends the dictation whatever the meter says. It is longer than
/// the meter's pause because it has to survive a mid-sentence breath.
const STILL: u64 = PER_MS * 6_000;
/// Nobody dictates for five minutes. Past that, put in what there is rather
/// than hold the microphone open for a machine that was left listening.
const LONGEST: i64 = 5 * 60 * 1_000;
/// About -42 dBFS. Speech sits above this; a quiet room sits below.
const FLOOR: f32 = 0.008;

/// A twentieth of a second of 16-bit mono audio.
const FRAME: usize = 16_000 / 50 * 2;

/// How loud the loudest moment in a stretch of audio is, 0 to 1. Averaged
/// over the whole stretch instead, the pauses between words would drag a
/// sentence down towards the level of a silent room and the two would stop
/// being tellable apart; the loudest moment keeps them far apart.
fn level(pcm: &[u8]) -> f32 {
    let mut loudest = 0f32;
    for frame in pcm.chunks(FRAME) {
        let n = frame.len() / 2;
        if n == 0 {
            continue;
        }
        let mut sum = 0f64;
        for c in frame.chunks_exact(2) {
            let s = i16::from_le_bytes([c[0], c[1]]) as f64 / 32_768.0;
            sum += s * s;
        }
        loudest = loudest.max((sum / n as f64).sqrt() as f32);
    }
    loudest
}

/// Given nothing but room tone, the speech model does not return nothing: it
/// invents a sentence, and a different one each time, which fills the card
/// with words nobody said and hides the fact that the speaking has stopped.
/// So silence is recognised here and never reaches it. The floor is absolute
/// and also relative, because a quiet microphone puts all of someone's
/// speech below any fixed line, and what matters then is that the pauses are
/// far quieter than the talking.
fn is_quiet(lvl: f32, loudest: f32) -> bool {
    if lvl < FLOOR && (loudest <= 0.0 || lvl < loudest * 0.35) {
        return true;
    }
    // A room can be noisy enough that nothing in it is ever quiet by any
    // fixed measure. What still holds is that a pause is far below the
    // talking: once speech has clearly been heard, a stretch this far under
    // the loudest of it is a pause, not a word. Without this a dictation in
    // a café would hold the microphone open until the five minutes ran out.
    loudest > FLOOR && lvl < loudest * 0.2
}

/// Dictation audio lives in a temporary folder that is deleted the moment
/// the words go in. A session cut short by a crash or a quit leaves the
/// recording behind, so anything from an earlier run goes at startup: a
/// recording of someone's voice should not outlive the sentence it was.
pub fn sweep_leftovers() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return };
    let keep = crate::lock(&SESSION).clone();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_ours = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("lane-dictation-"))
            .unwrap_or(false);
        if is_ours && Some(&path) != keep.as_ref() && std::fs::remove_dir_all(&path).is_ok() {
            log::info!("dictation: cleared a recording left behind by an earlier run");
        }
    }
}

pub fn live_text() -> String {
    crate::lock(&LIVE).0.clone()
}

fn spawn_live(app: AppHandle, state: Arc<AppState>, dir: PathBuf, started_at: i64) {
    *crate::lock(&LIVE) = (String::new(), 0);
    let Some(whisper) = crate::meetings::whisper_path(state.resource_dir.as_deref()) else { return };
    let model = crate::engine::speech_model_path(&state);
    if !model.is_file() {
        log::warn!("dictation: no speech model at {}, so nothing can be read back", model.display());
        return;
    }
    let lang = {
        let l = crate::lock(&state.settings).meeting_language.clone();
        if l.is_empty() { "auto".to_string() } else { l }
    };

    std::thread::Builder::new()
        .name("dictation-live".into())
        .spawn(move || {
            let wav = dir.join("mic.wav");
            let scratch = dir.join("live.wav");
            // What is settled, and how much audio it covers. Everything after
            // `base` is what gets re-read each pass.
            let mut settled = String::new();
            let mut base: u64 = 0;
            // Audio read since the words last changed.
            let mut still: u64 = 0;
            // The loudest stretch heard so far, which sets what counts as a
            // pause on this microphone.
            let mut loudest: f32 = 0.0;
            loop {
                std::thread::sleep(PACE);
                if crate::lock(&SESSION).is_none() {
                    break;
                }
                // Refused the microphone, macOS hands over silence rather
                // than an error, so there is nothing to hear and no reason
                // to keep the recorder open pretending otherwise.
                if crate::meetings::MIC_DENIED.load(std::sync::atomic::Ordering::Relaxed) {
                    log::warn!("dictation: the microphone is refused, stopping");
                    let app = app.clone();
                    let state = Arc::clone(&state);
                    std::thread::spawn(move || {
                        let _ = finish(&app, &state);
                        crate::engine::microphone_refused(&app);
                    });
                    break;
                }
                let Ok(meta) = std::fs::metadata(&wav) else { continue };
                let start = crate::meetings::pcm_offset(&wav);
                let avail = meta.len().saturating_sub(start) & !1;
                let covered = crate::lock(&LIVE).1;

                // Nothing new worth reading, so look at whether the room has
                // gone quiet instead.
                if avail < MIN_AUDIO || avail <= covered {
                    if should_finish(&state, started_at, still) {
                        let app = app.clone();
                        let state = Arc::clone(&state);
                        std::thread::spawn(move || {
                            if let Err(e) = finish(&app, &state) {
                                log::warn!("dictation: finishing by itself: {e}");
                            }
                        });
                        break;
                    }
                    continue;
                }

                // Is there anything in the new audio, or has the speaking
                // stopped? Only the part not yet read is judged.
                let Ok(fresh) = crate::meetings::read_range(&wav, start + covered, avail - covered) else { continue };
                let lvl = level(&fresh);
                loudest = loudest.max(lvl);
                if is_quiet(lvl, loudest) {
                    still += avail - covered;
                    crate::lock(&LIVE).1 = avail;
                    if should_finish(&state, started_at, still) {
                        let app = app.clone();
                        let state = Arc::clone(&state);
                        std::thread::spawn(move || {
                            if let Err(e) = finish(&app, &state) {
                                log::warn!("dictation: finishing by itself: {e}");
                            }
                        });
                        break;
                    }
                    continue;
                }

                // There is sound in it, so the speaking has not stopped.
                still = 0;

                // A copy with a header of its own: the live file is still
                // being written and its own header is not finished yet.
                let Ok(pcm) = crate::meetings::read_range(&wav, start + base, avail - base) else { continue };
                if crate::meetings::write_wav(&scratch, &pcm).is_err() {
                    continue;
                }
                match crate::meetings::transcribe(&whisper, &model, &scratch, &lang) {
                    Ok(segs) => {
                        let heard: String = segs
                            .iter()
                            .map(|s| s.text.trim())
                            .filter(|t| !t.is_empty())
                            .collect::<Vec<_>>()
                            .join(" ");
                        if !heard.is_empty() {
                            let text = join(&settled, &heard);
                            *crate::lock(&LIVE) = (text.clone(), avail);
                            crate::lock(&state.recording).live_text = text.clone();
                            let _ = app.emit("dictation-live", serde_json::json!({ "text": text }));
                        } else {
                            crate::lock(&LIVE).1 = avail;
                        }

                        // Past the window, put aside the sentences that have
                        // stopped changing. The cut falls between segments,
                        // which is where the speaker paused, so no word is
                        // split across the seam.
                        if avail - base > WINDOW {
                            let cut = ((avail - base) / PER_MS) as i64 - KEEP_MS;
                            let mut done: Vec<&str> = Vec::new();
                            let mut upto = 0i64;
                            for seg in &segs {
                                if seg.end_ms > cut {
                                    break;
                                }
                                done.push(seg.text.trim());
                                upto = seg.end_ms;
                            }
                            if !done.is_empty() {
                                settled = join(&settled, &done.join(" "));
                                base += upto as u64 * PER_MS;
                            } else if avail - base > WINDOW * 2 {
                                // One unbroken stretch of speech with nowhere
                                // to cut. Keep the last few seconds and carry
                                // on rather than getting slower every pass.
                                settled = join(&settled, &heard);
                                base = avail.saturating_sub(KEEP_MS as u64 * PER_MS);
                                log::debug!("dictation: no pause to cut at, seam forced");
                            }
                        }
                    }
                    Err(e) => log::debug!("dictation: live pass: {e}"),
                }
                let _ = std::fs::remove_file(&scratch);

                if should_finish(&state, started_at, still) {
                    let app = app.clone();
                    let state = Arc::clone(&state);
                    std::thread::spawn(move || {
                        if let Err(e) = finish(&app, &state) {
                            log::warn!("dictation: finishing by itself: {e}");
                        }
                    });
                    break;
                }
            }
        })
        .expect("spawn dictation live");
}

/// Put two pieces of speech together with one space between them.
fn join(before: &str, after: &str) -> String {
    if before.is_empty() {
        return after.trim().to_string();
    }
    if after.trim().is_empty() {
        return before.to_string();
    }
    format!("{} {}", before.trim_end(), after.trim())
}

/// Something has been said, and either the room has gone quiet or the words
/// have stopped arriving.
fn should_finish(state: &AppState, started_at: i64, still: u64) -> bool {
    let said = !crate::lock(&LIVE).0.trim().is_empty();
    let last = crate::lock(&state.recording).last_sound_at;
    let now = crate::capture::now_ms();
    // The cap does not care whether anything was said: a microphone left
    // open having heard nothing is exactly the case it is there for.
    if now - started_at >= LONGEST {
        log::info!("dictation: five minutes, putting in what there is");
        return true;
    }
    quiet_enough(now, started_at, last, said) || (said && still >= STILL)
}

/// Only sound heard since this dictation began counts. A timestamp from an
/// earlier recording would otherwise look like a long silence and put the
/// first words in before the sentence was finished.
fn quiet_enough(now: i64, started_at: i64, last_sound: Option<i64>, said: bool) -> bool {
    if !said {
        return false;
    }
    match last_sound {
        Some(t) if t >= started_at => now - t >= HUSH,
        _ => false,
    }
}

pub fn toggle(app: &AppHandle, state: &AppState) -> Result<String, String> {
    if crate::lock(&SESSION).is_some() {
        return finish(app, state);
    }
    if FINISHING.load(std::sync::atomic::Ordering::SeqCst) {
        // The words are on their way in. Pressing again here means "finish",
        // which is already happening, not "start another one".
        return Ok("finishing".into());
    }
    if crate::lock(&crate::meetings::RECORDER).is_some() {
        return Err("A meeting is being recorded; stop it first".into());
    }
    let (ready, why) = crate::engine::meeting_readiness(state);
    if !ready {
        return Err(why);
    }
    // Without the speech model there is nothing to turn the sound into
    // words. Recording anyway would hold the microphone open and produce
    // silence, which is what it did: dictation started, wrote a file nobody
    // could read, and said nothing at all.
    if !crate::engine::speech_model_path(state).is_file() {
        if let Some(shared) = app.try_state::<Arc<AppState>>() {
            crate::engine::fetch_speech_model(app, shared.inner().clone(), true);
        }
        return Err("Dictation is not ready yet. The model it needs is coming down — the progress is next to Capture, and dictation works the moment it lands.".into());
    }
    let helper = crate::meetings::helper_path(state.resource_dir.as_deref()).ok_or("recorder missing")?;
    let started_at = crate::capture::now_ms();
    let dir = std::env::temp_dir().join(format!("lane-dictation-{started_at}"));
    crate::meetings::start(&helper, &dir, 0, started_at, state.recording.clone(), true)?;
    *crate::lock(&SESSION) = Some(dir.clone());
    announce(app, true);
    // The live pass needs to outlive this call, so it takes the shared
    // state rather than the borrow.
    if let Some(shared) = app.try_state::<Arc<AppState>>() {
        spawn_live(app.clone(), shared.inner().clone(), dir, started_at);
    }
    crate::engine::notch(app, "dictation", "Listening", vec!["Speak. The words appear as you say them, and go in when you stop.".into()]);
    log::info!("dictation: started");
    Ok("listening".into())
}

fn finish(app: &AppHandle, state: &AppState) -> Result<String, String> {
    let dir = crate::lock(&SESSION).take().ok_or("not dictating")?;
    let _finishing = Finishing::start();
    let _ = crate::meetings::stop();
    {
        let mut r = crate::lock(&state.recording);
        r.recording = false;
        r.live_text.clear();
    }
    // The live pass has usually already read everything that was said. When
    // it has, the words go in the moment you stop talking instead of after
    // another trip through the speech engine, which is most of what makes
    // this feel immediate. Only the last stretch of audio is re-read.
    let wav = dir.join("mic.wav");
    let recorded = std::fs::metadata(&wav).map(|m| m.len().saturating_sub(crate::meetings::pcm_offset(&wav))).unwrap_or(0);
    let (live, covered) = crate::lock(&LIVE).clone();
    let fresh = !live.trim().is_empty() && recorded.saturating_sub(covered) < MIN_AUDIO;

    // Whatever happens from here on, the words that were heard go in and the
    // card stops saying it is listening. A last pass that will not run is a
    // reason to use what is already on screen, not a reason to lose it.
    let text = if fresh {
        live
    } else if !crate::engine::speech_model_path(state).is_file() {
        String::new()
    } else {
        match read_all(state, &wav) {
            Ok(t) if !t.trim().is_empty() => t,
            Ok(_) => live,
            Err(e) => {
                log::warn!("dictation: last pass: {e}; keeping what was heard");
                live
            }
        }
    };
    // Said before the folder goes, because when nothing comes back this is
    // the only account of why. A dictation that hears nothing used to leave
    // no trace at all, and the answer took an hour of picking over files.
    log::info!(
        "dictation: {:.1}s of sound, loudest {:.4}, {} characters",
        recorded as f64 / 32_000.0,
        loudest_in(&wav),
        text.chars().count(),
    );
    let _ = std::fs::remove_dir_all(&dir);
    *crate::lock(&LIVE) = (String::new(), 0);
    announce(app, false);
    if text.is_empty() {
        crate::engine::notch(
            app,
            "help",
            "Nothing was heard",
            vec!["The microphone was open but no speech came through. Check Lane is allowed the microphone in System Settings.".into()],
        );
        return Ok("empty".into());
    }
    insert_text(&text)?;
    // Dictated words are worth remembering too: a voice note, made into a memory.
    if let Err(e) = crate::lock(&state.store).create_note(&text, crate::capture::now_ms(), "Voice note") {
        log::warn!("dictation: could not keep the note: {e}");
    } else {
        state.engine_wake.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // Say both things. If the cursor was somewhere that does not take text,
    // the words are still in Lane, and someone who only sees "Inserted"
    // reasonably concludes they are gone.
    crate::engine::notch(
        app,
        "help",
        "Put in, and kept in Lane",
        vec![text.chars().take(160).collect(), "Saved as a voice note, whether or not your cursor took it.".into()],
    );
    let _ = app.emit("dictation-done", serde_json::json!({"text": text}));
    log::info!("dictation: inserted {} chars", text.chars().count());
    Ok(text)
}

/// The loudest moment in what was recorded, for the log. Cheap: it reads
/// the file once and keeps nothing.
fn loudest_in(wav: &std::path::Path) -> f32 {
    let start = crate::meetings::pcm_offset(wav);
    let len = std::fs::metadata(wav).map(|m| m.len().saturating_sub(start)).unwrap_or(0);
    match crate::meetings::read_range(wav, start, len.min(32_000 * 120)) {
        Ok(pcm) => level(&pcm),
        Err(_) => 0.0,
    }
}

/// One pass over everything that was recorded, for when the live text has
/// fallen behind the end of the sentence.
fn read_all(state: &AppState, wav: &std::path::Path) -> Result<String, String> {
    let whisper = crate::meetings::whisper_path(state.resource_dir.as_deref()).ok_or("speech engine missing")?;
    let model = state.db_path.with_file_name("models").join(crate::meetings::whisper_model().file);
    let lang = crate::lock(&state.settings).meeting_language.clone();
    let segs = crate::meetings::transcribe(&whisper, &model, wav, if lang.is_empty() { "auto" } else { &lang })?;
    Ok(segs.iter().map(|s| s.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" "))
}

/// Put the text where the cursor is: clipboard, then ⌘V, then the old
/// clipboard back. Needs Accessibility, which capture already has.
pub fn insert_text(text: &str) -> Result<(), String> {
    let mut board = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let previous = board.get_text().ok();
    board.set_text(text.to_string()).map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(80));
    crate::capture::platform::press_paste();
    // An app reads the clipboard when it gets round to handling the
    // keystroke, which under load is not immediately. Put the old clipboard
    // back too soon and that is what lands instead of the words, or nothing
    // does. So: wait long enough for a busy app, and only put it back if
    // nothing else has taken the clipboard in the meantime.
    std::thread::sleep(RESTORE_AFTER);
    if let Some(p) = previous {
        if board.get_text().map(|now| now == text).unwrap_or(false) {
            let _ = board.set_text(p);
        }
    }
    Ok(())
}

/// How long to leave the words on the clipboard before putting back what was
/// there before.
const RESTORE_AFTER: std::time::Duration = std::time::Duration::from_millis(900);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dictation_is_still_going_while_the_words_go_in() {
        assert!(!is_active());
        {
            let _busy = Finishing::start();
            assert!(is_active(), "the shortcut must not start a second one here");
        }
        assert!(!is_active());
    }

    #[test]
    fn join_keeps_one_space_and_tolerates_gaps() {
        assert_eq!(join("", "  hello "), "hello");
        assert_eq!(join("hello", "   "), "hello");
        assert_eq!(join("hello ", " there"), "hello there");
    }

    fn tone(samples: usize, amplitude: f32) -> Vec<u8> {
        let mut v = Vec::with_capacity(samples * 2);
        for i in 0..samples {
            let s = ((i as f32 * 0.3).sin() * amplitude * 32_767.0) as i16;
            v.extend_from_slice(&s.to_le_bytes());
        }
        v
    }

    #[test]
    fn loudness_is_measured_from_the_samples() {
        assert_eq!(level(&[]), 0.0);
        assert!(level(&tone(1_000, 0.0)) < 0.0001);
        let quiet = level(&tone(1_000, 0.01));
        let loud = level(&tone(1_000, 0.4));
        assert!(quiet < FLOOR && loud > FLOOR, "quiet {quiet}, loud {loud}");
    }

    #[test]
    fn a_sentence_is_judged_by_its_words_not_by_its_pauses() {
        // Two seconds of speech with long gaps in it, then two seconds of a
        // silent room. Averaged, the two nearly meet; by the loudest moment
        // they are far apart, which is what tells them apart.
        let mut speech = Vec::new();
        for i in 0..20 {
            speech.extend(tone(1_600, if i % 4 == 0 { 0.25 } else { 0.0005 }));
        }
        let room = tone(32_000, 0.0009);
        let talking = level(&speech);
        let empty = level(&room);
        assert!(talking > FLOOR, "talking {talking}");
        assert!(!is_quiet(talking, talking));
        assert!(is_quiet(empty, talking), "empty {empty} against {talking}");
    }

    #[test]
    fn a_pause_is_quiet_next_to_the_talking_around_it() {
        // Plain silence against ordinary speech.
        assert!(is_quiet(0.001, 0.2));
        assert!(!is_quiet(0.2, 0.2));
        // A microphone so quiet that all of the speech is under the fixed
        // floor: the pauses are still far below the talking.
        assert!(is_quiet(0.0008, 0.006));
        assert!(!is_quiet(0.006, 0.006));
        // Nothing heard yet, so the fixed floor is all there is to go on.
        assert!(is_quiet(0.001, 0.0));
    }

    #[test]
    fn a_noisy_room_still_has_pauses_in_it() {
        // A café: the background is well above any fixed floor, but it is
        // still far below the person talking.
        assert!(is_quiet(0.02, 0.2));
        assert!(!is_quiet(0.09, 0.2));
        // Quiet does not mean quiet just because the room is loud: with
        // nothing above the floor heard yet, the relative rule stays off.
        assert!(!is_quiet(0.005, 0.006));
    }

    #[test]
    fn a_pause_is_long_enough_to_think_in() {
        // Six seconds of silence, not three and a half, and the level
        // meter's pause is four and a half rather than under two.
        assert_eq!(STILL / PER_MS, 6_000);
        assert_eq!(HUSH, 4_500);
    }

    #[test]
    fn silence_from_an_earlier_recording_does_not_count() {
        let start = 1_000_000;
        // Nothing for seven seconds, in this dictation: finished.
        assert!(quiet_enough(start + 10_000, start, Some(start + 3_000), true));
        // A two second pause is thinking, not stopping.
        assert!(!quiet_enough(start + 10_000, start, Some(start + 8_000), true));
        // The only sound on record is from before this dictation began.
        assert!(!quiet_enough(start + 10_000, start, Some(start - 60_000), true));
        // Nothing heard at all.
        assert!(!quiet_enough(start + 10_000, start, None, true));
        // Nothing said yet, however long the quiet.
        assert!(!quiet_enough(start + 90_000, start, Some(start + 100), false));
    }

    #[test]
    fn the_window_holds_about_twenty_seconds() {
        assert_eq!(WINDOW / PER_MS / 1_000, 20);
        // A cut is only looked for once there is more than the window, and
        // it always leaves the last few seconds live.
        let avail = WINDOW + PER_MS * 1_000;
        let cut = (avail / PER_MS) as i64 - KEEP_MS;
        assert_eq!(cut, 17_000);
    }
}
