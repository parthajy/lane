//! Meetings: record what you hear and say, transcribe on this Mac, and turn
//! the transcript into memories like anything else.
//!
//! Recording is explicit (start/stop) with a visible indicator; nothing
//! listens on its own. Audio goes to a temporary folder and is deleted
//! after transcription unless the user chooses to keep it.
//!
//! Two streams: the Mac's audio output ("Others") and the microphone
//! ("You"), transcribed separately and merged by time, which labels
//! speakers without any voice profiling.

use crate::runtime::ModelSpec;
use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

/// Speech models by RAM tier. The large-v3-turbo quant is about as fast as
/// small and markedly better on accented English and Hindi; it needs the
/// headroom of a 16 GB machine next to the language model.
pub const WHISPER_MODELS: &[ModelSpec] = &[
    ModelSpec {
        file: "ggml-small.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        bytes: 487_601_967,
        min_ram_gb: 0,
        label: "speech model",
    },
    ModelSpec {
        file: "ggml-large-v3-turbo-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        bytes: 574_041_195,
        min_ram_gb: 16,
        label: "speech model",
    },
];

/// The best speech model this Mac can run.
pub fn whisper_model() -> &'static ModelSpec {
    let ram = crate::runtime::physical_ram_gb();
    WHISPER_MODELS.iter().rev().find(|m| ram >= m.min_ram_gb).unwrap_or(&WHISPER_MODELS[0])
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub recording: bool,
    /// When the recorder last heard anything above the noise floor. Used to
    /// stop a recording nobody is talking into.
    pub last_sound_at: Option<i64>,
    pub meeting_id: Option<i64>,
    pub started_at: Option<i64>,
    pub mic_ok: bool,
    pub system_ok: bool,
    pub detail: String,
    /// Transcript so far, refreshed every ~30 s while recording.
    pub live_text: String,
}

pub struct Recorder {
    child: Child,
    pub dir: PathBuf,
    pub meeting_id: i64,
    pub started_at: i64,
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub static RECORDER: Mutex<Option<Recorder>> = Mutex::new(None);
/// Live transcription progress per stream: bytes of PCM already transcribed.
static LIVE_DONE: Mutex<[u64; 2]> = Mutex::new([0, 0]);
static LIVE_SEGMENTS: Mutex<Vec<Segment>> = Mutex::new(Vec::new());
const LIVE_EVERY: std::time::Duration = std::time::Duration::from_secs(30);
const LIVE_MIN_BYTES: u64 = 16_000 * 2 * 15; // 15 s of 16 kHz mono int16
const WAV_HEADER: u64 = 44;

/// Transcribe whatever new audio each stream has written since the last
/// tick and refresh `status.live_text`. Runs on its own thread until the
/// recorder is gone.
pub fn spawn_live(whisper: PathBuf, model: PathBuf, dir: PathBuf, language: String, status: Arc<Mutex<RecordingStatus>>) {
    *crate::lock(&LIVE_DONE) = [0, 0];
    crate::lock(&LIVE_SEGMENTS).clear();
    std::thread::Builder::new()
        .name("live-notes".into())
        .spawn(move || loop {
            std::thread::sleep(LIVE_EVERY);
            if crate::lock(&RECORDER).is_none() {
                break;
            }
            for (i, (file, speaker)) in [("mic.wav", "You"), ("system.wav", "Others")].iter().enumerate() {
                let path = dir.join(file);
                let Ok(meta) = std::fs::metadata(&path) else { continue };
                let done = crate::lock(&LIVE_DONE)[i];
                let avail = meta.len().saturating_sub(WAV_HEADER);
                if avail.saturating_sub(done) < LIVE_MIN_BYTES {
                    continue;
                }
                // Cut at a whole sample and write a self-contained WAV.
                let take = (avail - done) & !1;
                let Ok(pcm) = read_range(&path, WAV_HEADER + done, take) else { continue };
                let tmp = dir.join(format!("live-{i}.wav"));
                if write_wav(&tmp, &pcm).is_err() {
                    continue;
                }
                let offset_ms = (done / 32) as i64; // 32 bytes per ms at 16 kHz int16
                if let Ok(segs) = transcribe(&whisper, &model, &tmp, &language) {
                    let mut all = crate::lock(&LIVE_SEGMENTS);
                    for mut s in segs {
                        s.speaker = speaker.to_string();
                        s.start_ms += offset_ms;
                        s.end_ms += offset_ms;
                        all.push(s);
                    }
                    all.sort_by_key(|s| s.start_ms);
                    let text = render(&join_turns(all.clone()));
                    crate::lock(&status).live_text = text;
                }
                let _ = std::fs::remove_file(&tmp);
                crate::lock(&LIVE_DONE)[i] = done + take;
            }
        })
        .expect("spawn live notes");
}

fn read_range(path: &Path, from: u64, len: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(from))?;
    let mut buf = vec![0u8; len as usize];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

fn write_wav(path: &Path, pcm: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    let data_len = pcm.len() as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&16_000u32.to_le_bytes())?;
    f.write_all(&32_000u32.to_le_bytes())?; // byte rate
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    f.write_all(pcm)?;
    Ok(())
}

/// Join consecutive segments of the same speaker (already labelled).
fn join_turns(all: Vec<Segment>) -> Vec<Segment> {
    let mut turns: Vec<Segment> = Vec::new();
    for s in all {
        match turns.last_mut() {
            Some(t) if t.speaker == s.speaker && s.start_ms - t.end_ms < 3_000 => {
                t.text.push(' ');
                t.text.push_str(&s.text);
                t.end_ms = t.end_ms.max(s.end_ms);
            }
            _ => turns.push(s),
        }
    }
    turns
}
pub static MIC_FRAMES: AtomicI64 = AtomicI64::new(0);
pub static SYSTEM_FRAMES: AtomicI64 = AtomicI64::new(0);

/// The recording helper: `Contents/Resources/audio/lane-audio` in the
/// app, `src-tauri/audio/lane-audio` in development.
pub fn helper_path(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(r) = resource_dir {
        candidates.push(r.join("audio").join("lane-audio"));
    }
    candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/audio/lane-audio")));
    candidates.into_iter().find(|p| p.is_file())
}

/// whisper.cpp's CLI: bundled (built statically with Metal embedded), the
/// development checkout, then a Homebrew install as a last resort.
pub fn whisper_path(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(r) = resource_dir {
        candidates.push(r.join("whisper").join("whisper-cli"));
    }
    candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/whisper/whisper-cli")));
    candidates.push(PathBuf::from("/opt/homebrew/opt/whisper-cpp/bin/whisper-cli"));
    candidates.push(PathBuf::from("/usr/local/opt/whisper-cpp/bin/whisper-cli"));
    candidates.into_iter().find(|p| p.is_file())
}

/// Start the helper writing into `dir`. Its JSON lines are followed on a
/// thread so the UI can show that both streams are live.
pub fn start(helper: &Path, dir: &Path, meeting_id: i64, started_at: i64, status: Arc<Mutex<RecordingStatus>>, mic_only: bool) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(helper);
    cmd.arg(dir);
    if mic_only {
        cmd.arg("--no-system");
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start the recorder: {e}"))?;
    let stdout = child.stdout.take().ok_or("recorder has no output")?;
    MIC_FRAMES.store(0, Ordering::Relaxed);
    SYSTEM_FRAMES.store(0, Ordering::Relaxed);
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            let mut st = crate::lock(&status);
            match v["event"].as_str() {
                Some("mic") => {
                    st.mic_ok = v["state"] == "recording";
                    if let Some(m) = v["message"].as_str() {
                        st.detail = format!("microphone: {m}");
                    }
                }
                Some("system") => {
                    st.system_ok = v["state"] == "recording";
                    if let Some(m) = v["message"].as_str() {
                        st.detail = format!("system audio: {m}");
                    }
                }
                Some("tick") | Some("stopped") => {
                    MIC_FRAMES.store(v["micFrames"].as_i64().unwrap_or(0), Ordering::Relaxed);
                    SYSTEM_FRAMES.store(v["systemFrames"].as_i64().unwrap_or(0), Ordering::Relaxed);
                    // Anything above a whisper counts as sound; room tone sits
                    // well below this.
                    let peak = v["micPeak"].as_f64().unwrap_or(0.0).max(v["systemPeak"].as_f64().unwrap_or(0.0));
                    if peak > 0.02 {
                        st.last_sound_at = Some(crate::capture::now_ms());
                    }
                }
                _ => {}
            }
        }
    });
    *crate::lock(&RECORDER) = Some(Recorder { child, dir: dir.to_path_buf(), meeting_id, started_at });
    Ok(())
}

/// Stop the helper (it finalises the WAV headers on SIGTERM) and return the
/// recording folder.
pub fn stop() -> Option<(PathBuf, i64, i64)> {
    let mut guard = crate::lock(&RECORDER);
    let mut rec = guard.take()?;
    #[cfg(unix)]
    unsafe {
        libc::kill(rec.child.id() as i32, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    let _ = rec.child.kill();
    let _ = rec.child.wait();
    let out = (rec.dir.clone(), rec.meeting_id, rec.started_at);
    // Already waited; prevent Drop from killing again (harmless, but tidy).
    std::mem::forget(rec);
    Some(out)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub speaker: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// Put the sizes back into a WAV header that was never finalised.
///
/// A recorder writes the RIFF and data lengths when it closes the file. If it
/// is killed first, the header still claims whatever it was when the file was
/// opened, and every reader sees a few kilobytes of a file that is megabytes
/// long. whisper then refuses it, and, unhelpfully, still exits zero.
///
/// The audio itself is intact, so the fix is to write the two lengths the
/// file actually has. Returns true when it repaired something.
pub fn repair_wav(path: &Path) -> bool {
    let Ok(mut b) = std::fs::read(path) else { return false };
    let size = b.len();
    if size < 44 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return false;
    }
    let get = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
    let declared = get(&b, 4);
    if declared == size - 8 {
        return false; // already sound
    }

    let mut i = 12;
    while i + 8 <= size {
        let id = [b[i], b[i + 1], b[i + 2], b[i + 3]];
        let len = get(&b, i + 4);
        if &id == b"data" {
            let real = (size - (i + 8)) as u32;
            b[4..8].copy_from_slice(&((size - 8) as u32).to_le_bytes());
            b[i + 4..i + 8].copy_from_slice(&real.to_le_bytes());
            if std::fs::write(path, &b).is_err() {
                return false;
            }
            log::info!("audio: repaired the header of {} ({} bytes of sound)", path.display(), real);
            return true;
        }
        // A chunk's length excludes its own header and is padded to even.
        i += 8 + len + (len & 1);
    }
    false
}

/// Run whisper-cli on one WAV file. Returns timed segments.
pub fn transcribe(whisper: &Path, model: &Path, wav: &Path, language: &str) -> Result<Vec<Segment>, String> {
    if !wav.is_file() || std::fs::metadata(wav).map(|m| m.len()).unwrap_or(0) < 16_000 {
        return Ok(vec![]);
    }
    repair_wav(wav);
    let out_base = wav.with_extension("");
    let status = Command::new(whisper)
        .args(["-m", &model.display().to_string(), "-f", &wav.display().to_string(), "-oj", "-of", &out_base.display().to_string(), "-l", language, "-np"])
        .args(["-t", "4", "-bs", "1", "-bo", "1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("could not run the speech model: {e}"))?;
    if !status.success() {
        return Err(format!("speech model failed ({status})"));
    }
    let json_path = out_base.with_extension("json");
    if !json_path.is_file() {
        // whisper-cli returns zero even when it fails to read the audio, so
        // a missing transcript is the only signal that it did not work. The
        // error used to surface as a bare "No such file or directory", which
        // says nothing about what actually went wrong.
        return Err(format!("the speech model could not read {}", wav.file_name().unwrap_or_default().to_string_lossy()));
    }
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&json_path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut segs = Vec::new();
    for s in v["transcription"].as_array().unwrap_or(&vec![]) {
        let text = s["text"].as_str().unwrap_or("").trim().to_string();
        if text.is_empty() || is_hallucination(&text) {
            continue;
        }
        segs.push(Segment {
            speaker: String::new(),
            start_ms: s["offsets"]["from"].as_i64().unwrap_or(0).max(0),
            end_ms: s["offsets"]["to"].as_i64().unwrap_or(0).max(0),
            text,
        });
    }
    Ok(segs)
}

/// Whisper invents these on silence.
fn is_hallucination(text: &str) -> bool {
    let t = text.trim().trim_matches(|c: char| c == '.' || c == '!' || c == '[' || c == ']' || c == '(' || c == ')').to_lowercase();
    matches!(t.as_str(), "you" | "thank you" | "thanks for watching" | "blank_audio" | "silence" | "music" | "" )
        || t.starts_with("subtitles by") || t.starts_with("subs by")
}

/// Merge the two labelled streams by time and render the transcript as
/// text: one line per turn, speaker changes marked.
pub fn merge(mut mic: Vec<Segment>, mut system: Vec<Segment>) -> Vec<Segment> {
    for s in &mut mic {
        s.speaker = "You".into();
    }
    for s in &mut system {
        s.speaker = "Others".into();
    }
    let mut all: Vec<Segment> = mic.into_iter().chain(system).collect();
    all.sort_by_key(|s| s.start_ms);
    join_turns(all)
}

/// Like `merge`, with the other side split by voice: each system segment
/// takes the diarised speaker it overlaps most, numbered by first
/// appearance ("Speaker 1", "Speaker 2"…). Returns the turns and the labels
/// used. With no diarised speech, falls back to "Others".
pub fn merge_with_speakers(mic: Vec<Segment>, mut system: Vec<Segment>, diar: &[(i64, i64, usize)]) -> (Vec<Segment>, Vec<String>) {
    if diar.is_empty() {
        return (merge(mic, system), vec![]);
    }
    let mut order: Vec<usize> = Vec::new();
    for s in &mut system {
        let mut best: Option<(i64, usize)> = None;
        for &(a, b, who) in diar {
            let overlap = s.end_ms.min(b) - s.start_ms.max(a);
            if overlap > 0 && best.map_or(true, |(o, _)| overlap > o) {
                best = Some((overlap, who));
            }
        }
        match best {
            Some((_, who)) => {
                let n = match order.iter().position(|&x| x == who) {
                    Some(i) => i + 1,
                    None => {
                        order.push(who);
                        order.len()
                    }
                };
                s.speaker = format!("Speaker {n}");
            }
            None => s.speaker = "Others".into(),
        }
    }
    let mut mic = mic;
    for s in &mut mic {
        s.speaker = "You".into();
    }
    let mut all: Vec<Segment> = mic.into_iter().chain(system).collect();
    all.sort_by_key(|s| s.start_ms);
    let labels = (1..=order.len()).map(|n| format!("Speaker {n}")).collect();
    (join_turns(all), labels)
}

pub fn render(turns: &[Segment]) -> String {
    turns
        .iter()
        .map(|t| format!("[{:02}:{:02}] {}: {}", t.start_ms / 60_000, (t.start_ms / 1000) % 60, t.speaker, t.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Cut a transcript into snapshot-sized pieces on turn boundaries.
pub fn chunks(text: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        if !cur.is_empty() && cur.len() + line.len() + 1 > max_chars {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(line);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: i64, end: i64, text: &str) -> Segment {
        Segment { speaker: String::new(), start_ms: start, end_ms: end, text: text.into() }
    }

    #[test]
    fn merge_labels_and_joins_turns() {
        let mic = vec![seg(0, 2000, "Hi Sarah,"), seg(2100, 4000, "can you send the tender?"), seg(12_000, 13_000, "Great.")];
        let system = vec![seg(5000, 9000, "Yes, by Friday."), seg(9200, 11_000, "I'll copy Tom.")];
        let turns = merge(mic, system);
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].speaker, "You");
        assert_eq!(turns[0].text, "Hi Sarah, can you send the tender?");
        assert_eq!(turns[1].speaker, "Others");
        assert_eq!(turns[1].text, "Yes, by Friday. I'll copy Tom.");
        let text = render(&turns);
        assert!(text.starts_with("[00:00] You: Hi Sarah"));
        assert!(text.contains("[00:05] Others:"));
        assert_eq!(chunks(&text, 60).len(), 3);
    }

    #[test]
    fn wav_writer_produces_readable_header() {
        let dir = std::env::temp_dir().join(format!("rat-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.wav");
        write_wav(&p, &[0u8; 3200]).unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 3200);
        assert_eq!(read_range(&p, 44, 100).unwrap().len(), 100);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn whisper_silence_fillers_are_dropped() {
        assert!(is_hallucination(" you"));
        assert!(is_hallucination("[BLANK_AUDIO]"));
        assert!(is_hallucination("Thank you."));
        assert!(!is_hallucination("Thank you for the draft, Sarah."));
    }

    #[test]
    fn helper_and_whisper_are_found_in_dev_checkout() {
        assert!(helper_path(None).is_some(), "build with: swiftc -O -framework AVFoundation -framework CoreAudio -o audio/lane-audio audio/lane-audio.swift");
        // whisper-cli is optional in CI; only assert when Homebrew has it.
        if Path::new("/opt/homebrew/opt/whisper-cpp").exists() {
            assert!(whisper_path(None).is_some());
        }
    }
}

#[cfg(test)]
mod live {
    //! cargo test live_meeting -- --ignored --nocapture
    //! Records ~10 s (say something, or let the Mac speak), transcribes, prints turns.
    #[test]
    #[ignore]
    fn live_meeting() {
        use super::*;
        let dir = std::env::temp_dir().join(format!("rat-meeting-{}", std::process::id()));
        let helper = helper_path(None).unwrap();
        let whisper = whisper_path(None).unwrap();
        let model = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Library/Application Support/so.lane.app/models").join(whisper_model().file);
        let status = Arc::new(Mutex::new(RecordingStatus::default()));
        start(&helper, &dir, 1, 0, status.clone(), false).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(11));
        let (d, _, _) = stop().unwrap();
        eprintln!("status: {:?}", crate::lock(&status));
        let t = std::time::Instant::now();
        let mic = transcribe(&whisper, &model, &d.join("mic.wav"), "auto").unwrap();
        let sys = transcribe(&whisper, &model, &d.join("system.wav"), "auto").unwrap();
        eprintln!("transcribed in {:.1}s: mic {} segs, system {} segs", t.elapsed().as_secs_f64(), mic.len(), sys.len());
        let turns = merge(mic, sys);
        eprintln!("{}", render(&turns));
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod speaker_tests {
    use super::*;
    fn seg(a: i64, b: i64, t: &str) -> Segment {
        Segment { speaker: String::new(), start_ms: a, end_ms: b, text: t.into() }
    }
    #[test]
    fn other_side_gets_numbered_voices() {
        let mic = vec![seg(0, 1000, "hi")];
        let system = vec![seg(1200, 3000, "hello"), seg(3200, 5000, "yes"), seg(5200, 7000, "back")];
        let diar = vec![(1000, 3100, 7), (3100, 5100, 2), (5100, 7100, 7)];
        let (turns, labels) = merge_with_speakers(mic, system, &diar);
        assert_eq!(labels, vec!["Speaker 1", "Speaker 2"]);
        let names: Vec<&str> = turns.iter().map(|t| t.speaker.as_str()).collect();
        assert_eq!(names, vec!["You", "Speaker 1", "Speaker 2", "Speaker 1"]);
    }
}
