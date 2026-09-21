//! Bundled model runtime: a llama.cpp server shipped inside the app and a
//! model file downloaded once into the app data folder. Nothing else on
//! the user's Mac is needed.
//!
//! Network use, in full: the model download from a fixed URL on first run
//! (or when the model tier changes), and requests to the server on
//! 127.0.0.1. During development, an Ollama server on its default port is
//! used as a fallback when the bundled runtime is not present.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Model tiers by memory. The app picks by physical RAM; the user can
/// override the file name in settings.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub file: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub min_ram_gb: u64,
    pub label: &'static str,
}

pub const MODELS: &[ModelSpec] = &[
    // Non-thinking instruct build: extraction needs answers, not deliberation.
    ModelSpec {
        file: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        bytes: 2_497_281_120,
        min_ram_gb: 8,
        label: "Rabbit S (4B)",
    },
    // Hybrid model; the runtime is started with thinking disabled. Not yet
    // measured on a 16 GB Mac.
    ModelSpec {
        file: "Qwen3-8B-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q4_K_M.gguf",
        bytes: 5_027_784_512,
        min_ram_gb: 16,
        label: "Rabbit M (8B)",
    },
];

/// Embedding model for semantic search. Small enough to run on the CPU
/// alongside the chat model.
pub const EMBED_MODEL: ModelSpec = ModelSpec {
    file: "nomic-embed-text-v1.5.Q8_0.gguf",
    url: "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf",
    bytes: 146_146_432,
    min_ram_gb: 0,
    label: "search index",
};
pub const EMBED_DIM: usize = 768;

pub fn physical_ram_gb() -> u64 {
    #[cfg(target_os = "macos")]
    unsafe {
        let mut size: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let name = std::ffi::CString::new("hw.memsize").unwrap();
        if libc::sysctlbyname(name.as_ptr(), &mut size as *mut u64 as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) == 0 {
            return size / (1024 * 1024 * 1024);
        }
    }
    8
}

/// The largest tier this Mac can run.
pub fn default_model() -> &'static ModelSpec {
    let ram = physical_ram_gb();
    MODELS.iter().rev().find(|m| ram >= m.min_ram_gb).unwrap_or(&MODELS[0])
}

pub fn spec_for(file: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.file == file)
}

// ── Download ─────────────────────────────────────────────────────────────

pub struct DownloadProgress {
    pub done: AtomicU64,
    pub total: AtomicU64,
}

impl DownloadProgress {
    pub const fn new() -> Self {
        Self { done: AtomicU64::new(0), total: AtomicU64::new(0) }
    }
    pub fn percent(&self) -> u64 {
        let t = self.total.load(Ordering::Relaxed);
        if t == 0 { 0 } else { self.done.load(Ordering::Relaxed) * 100 / t }
    }
}

/// Download to `dest`, resuming a partial `.part` file if present.
pub fn download(spec: &ModelSpec, dest: &Path, progress: &DownloadProgress) -> Result<(), String> {
    let part = dest.with_extension("gguf.part");
    let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    progress.total.store(spec.bytes, Ordering::Relaxed);
    progress.done.store(have, Ordering::Relaxed);
    if have >= spec.bytes {
        std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mut req = ureq::get(spec.url).timeout(Duration::from_secs(3600));
    if have > 0 {
        req = req.set("Range", &format!("bytes={have}-"));
    }
    let resp = req.call().map_err(|e| format!("download failed: {e}"))?;
    let resuming = resp.status() == 206;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(resuming)
        .write(true)
        .truncate(!resuming)
        .open(&part)
        .map_err(|e| e.to_string())?;
    if !resuming {
        progress.done.store(0, Ordering::Relaxed);
    }
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        progress.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    file.flush().map_err(|e| e.to_string())?;
    let got = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if got < spec.bytes {
        return Err(format!("download incomplete: {got} of {} bytes", spec.bytes));
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

// ── Server process ───────────────────────────────────────────────────────

pub struct Server {
    child: Child,
    pub port: u16,
    pub model_file: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Where the bundled llama.cpp lives: `Lane.app/Contents/Resources/llama/`
/// in the installed app, `src-tauri/llama/` in development.
pub fn runtime_dir(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(r) = resource_dir {
        candidates.push(r.join("llama"));
    }
    if let Ok(exe) = std::env::current_exe() {
        // target/{debug,release}/rat-mac → src-tauri/llama (one level deeper for test binaries)
        for n in [3, 4] {
            if let Some(root) = exe.ancestors().nth(n) {
                candidates.push(root.join("llama"));
            }
        }
    }
    // The development checkout itself.
    candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/llama")));
    candidates.into_iter().find(|d| d.join("llama-server").is_file())
}

/// Kill runtime processes left behind by an earlier run of this app (a
/// crash, or a test binary exiting without dropping its servers). They
/// would otherwise hold gigabytes of GPU memory and starve the new one.
pub fn kill_strays(runtime_dir: &Path) {
    let pattern = runtime_dir.join("llama-server").display().to_string();
    let _ = Command::new("pkill").args(["-f", &pattern]).status();
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(18434)
}

pub fn start(runtime: &Path, model: &Path, log_path: &Path) -> Result<Server, String> {
    start_with(
        runtime,
        model,
        log_path,
        // Two slots so a question or briefing starts at once while a memory is
        // being made in the background; 8-bit KV cache keeps the doubled
        // context at ~0.6 GB on an 8 GB Mac.
        // --cache-reuse keeps the shared system prompt's KV between requests,
        // so a question only pays to process what is new.
        &["-c", "8192", "--parallel", "2", "-ctk", "q8_0", "-ctv", "q8_0", "-ngl", "99", "--jinja", "--reasoning-budget", "0", "-fa", "on", "--cache-reuse", "256"],
    )
}

/// Embedding server: nomic uses mean pooling and needs the whole input in
/// one batch.
pub fn start_embed(runtime: &Path, model: &Path, log_path: &Path) -> Result<Server, String> {
    start_with(
        runtime,
        model,
        log_path,
        &["--embedding", "--pooling", "mean", "-c", "2048", "-b", "2048", "-ub", "2048", "--parallel", "1", "-ngl", "0"],
    )
}

fn start_with(runtime: &Path, model: &Path, log_path: &Path, extra: &[&str]) -> Result<Server, String> {
    let port = free_port();
    let log = std::fs::OpenOptions::new().create(true).append(true).open(log_path).map_err(|e| e.to_string())?;
    let child = Command::new(runtime.join("llama-server"))
        .args(["-m", &model.display().to_string(), "--host", "127.0.0.1", "--port", &port.to_string(), "--no-webui", "--log-timestamps"])
        .args(extra)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| format!("could not start model runtime: {e}"))?;
    let mut server = Server { child, port, model_file: model.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default() };
    // Model load: ~1 min on an 8 GB M1 from a cold disk cache.
    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline {
        if let Ok(Some(status)) = server.child.try_wait() {
            return Err(format!("model runtime exited during startup ({status}); see llama.log"));
        }
        if health(port) {
            return Ok(server);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err("model runtime did not become ready in time".into())
}

pub fn health(port: u16) -> bool {
    ureq::get(&format!("http://127.0.0.1:{port}/health"))
        .timeout(Duration::from_secs(2))
        .call()
        .ok()
        .and_then(|r| r.into_json::<Value>().ok())
        .is_some_and(|v| v["status"] == "ok")
}

/// One structured chat completion against the bundled server. Returns the
/// Each slot has 4,096 tokens of context. Prompts are kept well under
/// that; a server 400 (too long) is retried once with the middle cut out.
/// Each server slot has 4096 tokens (`-c 8192 --parallel 2`); the system
/// prompt takes ~600 and the answer up to ~650, so the user part must stay
/// near 2,700 tokens. Mixed prose, numbers and URLs run ~3.2 chars a token.
const MAX_PROMPT_CHARS: usize = 7_000;

pub fn fit_prompt(user: &str, max_chars: usize) -> std::borrow::Cow<'_, str> {
    if user.chars().count() <= max_chars {
        return std::borrow::Cow::Borrowed(user);
    }
    let keep_head = max_chars * 6 / 10;
    let keep_tail = max_chars - keep_head - 40;
    let head: String = user.chars().take(keep_head).collect();
    let tail: String = user.chars().rev().take(keep_tail).collect::<Vec<_>>().into_iter().rev().collect();
    std::borrow::Cow::Owned(format!("{head}\n[… shortened …]\n{tail}"))
}

fn describe(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&body).ok().and_then(|v| v["error"]["message"].as_str().map(String::from)).unwrap_or(body);
            format!("model call failed: status code {code}: {}", msg.chars().take(200).collect::<String>())
        }
        other => format!("model call failed: {other}"),
    }
}

/// JSON text and generation speed in tokens/s.
pub fn chat_json(port: u16, system: &str, user: &str, schema: Value, max_tokens: u32) -> Result<(String, f64), String> {
    let user = fit_prompt(user, MAX_PROMPT_CHARS);
    let user = user.as_ref();
    let body = json!({
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "response_format": {"type": "json_schema", "json_schema": {"name": "memory", "schema": schema}},
        "temperature": 0.1,
        "max_tokens": max_tokens,
        "chat_template_kwargs": {"enable_thinking": false}
    });
    let resp = match ureq::post(&format!("http://127.0.0.1:{port}/v1/chat/completions")).timeout(Duration::from_secs(600)).send_json(body.clone()) {
        Ok(r) => r,
        Err(ureq::Error::Status(400, _)) => {
            // Too long for the slot: cut the middle and try once more.
            let shorter = fit_prompt(user, MAX_PROMPT_CHARS * 6 / 10);
            let mut body = body;
            body["messages"][1]["content"] = json!(shorter.as_ref());
            ureq::post(&format!("http://127.0.0.1:{port}/v1/chat/completions")).timeout(Duration::from_secs(600)).send_json(body).map_err(describe)?
        }
        Err(e) => return Err(describe(e)),
    };
    let v: Value = resp.into_json().map_err(|e| e.to_string())?;
    let content = v["choices"][0]["message"]["content"].as_str().ok_or("empty model response")?.to_string();
    let tps = v["timings"]["predicted_per_second"].as_f64().unwrap_or(0.0);
    Ok((content, tps))
}

/// Embed texts. Vectors come back unit-normalised.
pub fn embed(port: u16, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
    let resp = ureq::post(&format!("http://127.0.0.1:{port}/v1/embeddings"))
        .timeout(Duration::from_secs(120))
        .send_json(json!({"input": texts}))
        .map_err(|e| format!("embedding call failed: {e}"))?;
    let v: Value = resp.into_json().map_err(|e| e.to_string())?;
    let data = v["data"].as_array().ok_or("embedding response has no data")?;
    let mut out = Vec::with_capacity(data.len());
    for item in data {
        let mut vec: Vec<f32> = item["embedding"].as_array().ok_or("bad embedding")?.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
        if vec.len() != EMBED_DIM {
            return Err(format!("embedding has {} dimensions, expected {EMBED_DIM}", vec.len()));
        }
        let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            vec.iter_mut().for_each(|x| *x /= norm);
        }
        out.push(vec);
    }
    Ok(out)
}

/// Streamed chat completion: `on_token` is called for each text delta;
/// returns the full text.
pub fn chat_stream(port: u16, system: &str, user: &str, max_tokens: u32, mut on_token: impl FnMut(&str)) -> Result<String, String> {
    match chat_stream_once(port, system, user, max_tokens, MAX_PROMPT_CHARS, &mut on_token)? {
        (full, finish) if finish == "length" && (full.chars().count() < 200 || !full.trim_end().ends_with(|c: char| ".!?)]".contains(c))) => {
            // The prompt filled the slot and the answer was cut after a few
            // words: once more with a shorter prompt. The UI replaces the
            // streamed fragment with the final text.
            log::warn!("runtime: answer cut by the context window after {} chars; retrying with a shorter prompt", full.chars().count());
            on_token("\n\n");
            chat_stream_once(port, system, user, max_tokens, MAX_PROMPT_CHARS * 6 / 10, &mut on_token).map(|(t, _)| t)
        }
        (full, _) => Ok(full),
    }
}

/// One streamed completion; returns (text, finish reason).
fn chat_stream_once(port: u16, system: &str, user: &str, max_tokens: u32, max_chars: usize, on_token: &mut impl FnMut(&str)) -> Result<(String, String), String> {
    use std::io::BufRead;
    let user = fit_prompt(user, max_chars);
    let user = user.as_ref();
    let body = json!({
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "temperature": 0.2,
        "max_tokens": max_tokens,
        "stream": true,
        "chat_template_kwargs": {"enable_thinking": false}
    });
    let resp = match ureq::post(&format!("http://127.0.0.1:{port}/v1/chat/completions")).timeout(Duration::from_secs(600)).send_json(body.clone()) {
        Ok(r) => r,
        Err(ureq::Error::Status(400, _)) => {
            let shorter = fit_prompt(user, MAX_PROMPT_CHARS * 6 / 10);
            let mut body = body;
            body["messages"][1]["content"] = json!(shorter.as_ref());
            ureq::post(&format!("http://127.0.0.1:{port}/v1/chat/completions")).timeout(Duration::from_secs(600)).send_json(body).map_err(describe)?
        }
        Err(e) => return Err(describe(e)),
    };
    let reader = std::io::BufReader::new(resp.into_reader());
    let mut full = String::new();
    let mut reasoning = 0usize;
    let mut finish = String::new();
    let mut prompt_tokens = 0u64;
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Some(data) = line.strip_prefix("data: ") else { continue };
        if data.trim() == "[DONE]" {
            break;
        }
        let v: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
        let delta = &v["choices"][0]["delta"];
        if let Some(t) = delta["content"].as_str() {
            full.push_str(t);
            on_token(t);
        }
        if let Some(r) = delta["reasoning_content"].as_str() {
            reasoning += r.len();
        }
        if let Some(f) = v["choices"][0]["finish_reason"].as_str() {
            finish = f.to_string();
        }
        if let Some(p) = v["usage"]["prompt_tokens"].as_u64() {
            prompt_tokens = p;
        }
    }
    if full.trim().is_empty() {
        return Err(format!(
            "model produced no answer (finish: {finish:?}, reasoning chars: {reasoning}, prompt tokens: {prompt_tokens})"
        ));
    }
    Ok((full, finish))
}

/// Keeps at most one chat server and one embedding server alive.
pub static SERVER: Mutex<Option<Server>> = Mutex::new(None);
pub static EMBED_SERVER: Mutex<Option<Server>> = Mutex::new(None);

/// Stop both servers. Statics are not dropped at process exit, so tests
/// and quit paths call this explicitly.
pub fn shutdown() {
    *crate::lock(&SERVER) = None;
    *crate::lock(&EMBED_SERVER) = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_pick_by_ram() {
        assert!(physical_ram_gb() >= 4);
        assert!(MODELS.iter().all(|m| m.url.ends_with(m.file)));
        let d = default_model();
        assert!(physical_ram_gb() >= d.min_ram_gb);
        assert_eq!(spec_for("Qwen3-4B-Instruct-2507-Q4_K_M.gguf").map(|m| m.min_ram_gb), Some(8));
    }

    #[test]
    fn runtime_dir_is_found_in_dev_checkout_or_absent() {
        // Either the dev checkout ships src-tauri/llama or nothing is found;
        // both are valid, but a found dir must contain the server.
        if let Some(d) = runtime_dir(None) {
            assert!(d.join("llama-server").is_file());
        }
    }
}

#[cfg(test)]
mod live {
    //! RAT_MODEL_DIR=<dir> cargo test live_download -- --ignored --nocapture
    //! Downloads tier S into <dir> with the app's own downloader (resumable).
    #[test]
    #[ignore]
    fn live_download() {
        let dir = std::path::PathBuf::from(std::env::var("RAT_MODEL_DIR").expect("RAT_MODEL_DIR"));
        std::fs::create_dir_all(&dir).unwrap();
        let spec = &super::MODELS[0];
        let dest = dir.join(spec.file);
        let progress = super::DownloadProgress::new();
        let t = std::time::Instant::now();
        std::thread::scope(|sc| {
            sc.spawn(|| {
                while super::download(spec, &dest, &progress).is_err() {
                    eprintln!("retrying in 5s…");
                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
            });
            while !dest.is_file() {
                std::thread::sleep(std::time::Duration::from_secs(10));
                eprintln!("{}% after {:.0}s", progress.percent(), t.elapsed().as_secs_f64());
            }
        });
        eprintln!("done: {} bytes in {:.0}s", std::fs::metadata(&dest).unwrap().len(), t.elapsed().as_secs_f64());
    }

    #[test]
    fn long_prompts_are_shortened_in_the_middle() {
        let long: String = (0..20_000).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
        let fitted = super::fit_prompt(&long, 11_000);
        assert!(fitted.chars().count() <= 11_000 + 20);
        assert!(fitted.starts_with("abc") && fitted.ends_with(&long[long.len() - 10..]));
        assert!(fitted.contains("[… shortened …]"));
        assert_eq!(super::fit_prompt("short", 100).as_ref(), "short");
    }
}
