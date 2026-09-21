//! File index: what documents are on this Mac, where, and what they say.
//!
//! Text comes from the Mac's own Spotlight importers (`mdimport -t`), which
//! read PDF, Word, PowerPoint, Excel, Pages, Keynote, Numbers and more, with
//! `textutil` and plain reads as fallbacks. Nothing is uploaded; the index
//! lives in the same local database as memories.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

/// Documents the Mac's own importers can read.
pub const DOC_EXTS: &[&str] = &["pdf", "docx", "doc", "pptx", "ppt", "xlsx", "xls", "rtf", "pages", "key", "numbers", "odt", "ods", "odp", "epub"];
/// Anything that is already text: read straight off the disk.
pub const TEXT_EXTS: &[&str] = &[
    // notes and prose
    "txt", "md", "markdown", "rst", "adoc", "org", "tex", "csv", "tsv",
    // the web
    "html", "htm", "css", "scss", "svg",
    // code
    "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "rs", "go", "java", "kt", "kts", "swift",
    "m", "mm", "c", "h", "cc", "cpp", "hpp", "cs", "rb", "php", "pl", "lua", "dart", "scala",
    "ex", "exs", "erl", "hs", "clj", "vue", "svelte", "sh", "bash", "zsh", "fish", "sql", "r",
    // what a project says about itself
    "json", "yaml", "yml", "toml", "ini", "cfg", "conf", "xml", "plist", "gradle", "properties",
    "dockerfile", "makefile", "gitignore", "editorconfig",
    // what was said in a recording someone else made
    "srt", "vtt", "sbv",
    // patches and notes from the terminal
    "patch", "diff",
];
pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "heic", "tiff", "tif", "webp"];

/// Everything Lane will look inside.
pub fn is_indexable(ext: &str) -> bool {
    DOC_EXTS.contains(&ext) || TEXT_EXTS.contains(&ext) || IMAGE_EXTS.contains(&ext)
}

/// Files with no extension that are worth reading anyway.
const BARE_NAMES: &[&str] = &["makefile", "dockerfile", "readme", "license", "licence", "changelog", "notes", "todo", "procfile", "brewfile"];

/// Names that usually hold a secret, or machine noise nobody asks about.
fn is_unwanted(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    const SECRETS: &[&str] = &["id_rsa", "id_ed25519", "credentials", "secrets", "secret", ".pem", ".p12", ".keychain", ".netrc", ".npmrc", ".pgpass", "known_hosts", "authorized_keys"];
    const NOISE: &[&str] = &[".lock", "-lock.json", ".min.js", ".min.css", ".map", ".bundle.js", ".chunk.js", ".pyc", ".class", ".o", ".d.ts"];
    SECRETS.iter().any(|x| n.contains(x)) || NOISE.iter().any(|x| n.ends_with(x)) || n.starts_with("id_rsa")
}

static OCR_HELPER: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();

/// Where the bundled `lane-ocr` helper is; set once at startup.
pub fn set_ocr_helper(resource_dir: Option<&Path>) {
    let mut candidates = Vec::new();
    if let Some(r) = resource_dir {
        candidates.push(r.join("audio").join("lane-ocr"));
    }
    candidates.push(std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/audio/lane-ocr")));
    let _ = OCR_HELPER.set(candidates.into_iter().find(|p| p.is_file()));
}

/// Text in an image through the Mac's own Vision OCR (screenshots, scans, photos of whiteboards).
pub fn ocr_text(path: &str) -> Option<String> {
    let helper = OCR_HELPER.get().cloned().flatten()?;
    let out = Command::new(helper).arg(path).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    (!text.trim().is_empty()).then_some(text)
}
const MAX_BYTES: u64 = 25 * 1024 * 1024;
const MAX_CHARS: usize = 60_000;
pub const CHUNK_CHARS: usize = 1_200;
const SKIP_DIRS: &[&str] = &[
    "node_modules", "Library", "target", ".git", ".Trash", "venv", ".venv", "__pycache__",
    "dist", "build", "out", "vendor", "Pods", "DerivedData", ".next", ".nuxt", ".svelte-kit",
    ".cache", ".gradle", ".idea", ".tox", ".mypy_cache", ".pytest_cache", "coverage",
    "bower_components", "Carthage", ".terraform", "site-packages", ".cargo", ".rustup",
];
/// Plain text is cheap to read but a 40 MB log helps nobody.
const MAX_TEXT_BYTES: u64 = 3 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Candidate {
    pub path: String,
    pub size: u64,
    pub mtime: i64,
}

pub fn default_folders() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else { return vec![] };
    ["Desktop", "Documents", "Downloads"].iter().map(|d| PathBuf::from(&home).join(d).display().to_string()).collect()
}

/// Every indexable document under the folders. Cheap: metadata only.
pub fn scan(folders: &[String]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for folder in folders {
        let walker = WalkDir::new(folder).follow_links(false).max_depth(12).into_iter().filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(name.starts_with('.') || (e.file_type().is_dir() && SKIP_DIRS.contains(&name.as_ref())))
        });
        for entry in walker.filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if is_unwanted(&name) {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
            let bare = ext.is_empty() && BARE_NAMES.contains(&name.to_ascii_lowercase().as_str());
            if !bare && !is_indexable(ext.as_str()) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let cap = if bare || TEXT_EXTS.contains(&ext.as_str()) { MAX_TEXT_BYTES } else { MAX_BYTES };
            if meta.len() == 0 || meta.len() > cap {
                continue;
            }
            let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
            out.push(Candidate { path: path.display().to_string(), size: meta.len(), mtime });
        }
    }
    out
}

/// Text of a document, or None when nothing readable is in it.
pub fn extract_text(path: &str) -> Option<String> {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
    let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or_default().to_ascii_lowercase();
    let text = match ext.as_str() {
        e if IMAGE_EXTS.contains(&e) => ocr_text(path),
        e if TEXT_EXTS.contains(&e) => std::fs::read_to_string(path).ok(),
        "" if BARE_NAMES.contains(&name.as_str()) => std::fs::read_to_string(path).ok(),
        _ => mdimport_text(path).or_else(|| textutil_text(path)),
    };
    text.map(|t| tidy(&t)).filter(|t| t.chars().count() >= 40)
}

/// Spotlight's importer output includes `kMDItemTextContent = "…"` with
/// backslash escapes; read that value.
fn mdimport_text(path: &str) -> Option<String> {
    let out = Command::new("mdimport").args(["-t", "-d3", "-n", path]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let start = s.find("kMDItemTextContent = \"")? + "kMDItemTextContent = \"".len();
    let mut buf = String::new();
    let mut esc = false;
    for c in s[start..].chars() {
        if esc {
            buf.push(match c {
                'n' => '\n',
                't' => '\t',
                other => other,
            });
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == '"' {
            break;
        } else {
            buf.push(c);
        }
    }
    (!buf.trim().is_empty()).then_some(buf)
}

fn textutil_text(path: &str) -> Option<String> {
    let out = Command::new("textutil").args(["-stdout", "-convert", "txt", path]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    (!s.trim().is_empty()).then_some(s)
}

fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(MAX_CHARS));
    let mut blank = 0;
    for line in text.lines() {
        let l = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        if out.len() + l.len() + 1 > MAX_CHARS {
            break;
        }
        out.push_str(&l);
        out.push('\n');
    }
    out.trim().to_string()
}

/// Split on paragraph boundaries into pieces of about CHUNK_CHARS.
pub fn chunk(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if !cur.is_empty() && cur.len() + para.len() + 2 > CHUNK_CHARS {
            chunks.push(std::mem::take(&mut cur));
        }
        if para.len() > CHUNK_CHARS {
            // A huge paragraph: cut by sentences/lines, then by words.
            for piece in para.split_inclusive(|c| c == '\n' || c == '.') {
                for word in piece.split_inclusive(' ') {
                    if cur.len() + word.len() > CHUNK_CHARS && !cur.is_empty() {
                        chunks.push(std::mem::take(&mut cur));
                    }
                    cur.push_str(word);
                }
            }
            continue;
        }
        if !cur.is_empty() {
            cur.push_str("\n\n");
        }
        cur.push_str(para);
    }
    if !cur.trim().is_empty() {
        chunks.push(cur);
    }
    chunks
}

/// Watches the folders and flips `flag` whenever anything changes, so the
/// next idle pass rescans. Keeps the watcher alive for the process.
pub fn watch(folders: &[String], flag: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Option<notify::RecommendedWatcher> {
    use notify::{RecursiveMode, Watcher};
    let f = flag.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        if let Ok(ev) = res {
            if !ev.kind.is_access() {
                f.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    })
    .ok()?;
    for folder in folders {
        let _ = watcher.watch(Path::new(folder), RecursiveMode::Recursive);
    }
    Some(watcher)
}

pub fn reveal(path: &str) -> Result<(), String> {
    if path.starts_with("notion://") {
        return open_file(path);
    }
    Command::new("open").args(["-R", path]).spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn open_file(path: &str) -> Result<(), String> {
    if let Some(id) = path.strip_prefix("notion://") {
        let url = format!("https://www.notion.so/{}", id.replace('-', ""));
        return Command::new("open").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string());
    }
    Command::new("open").arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Extraction time guard for logging slow importers.
pub fn timed_extract(path: &str) -> (Option<String>, Duration) {
    let t = Instant::now();
    (extract_text(path), t.elapsed())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunking_respects_paragraphs_and_size() {
        let text = (0..60).map(|i| format!("Paragraph {i} with some words in it to make it longer than a few characters.")).collect::<Vec<_>>().join("\n\n");
        let chunks = chunk(&text);
        assert!(chunks.len() >= 3);
        assert!(chunks.iter().all(|c| c.len() <= CHUNK_CHARS + 100));
        assert!(chunks.iter().all(|c| !c.trim().is_empty()));
        let huge = "word ".repeat(1000);
        assert!(chunk(&huge).len() >= 3, "a single huge paragraph is still split");
    }

    #[test]
    fn plain_text_files_extract_and_tidy() {
        let dir = std::env::temp_dir().join(format!("rat-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("note.txt");
        std::fs::write(&p, "Hello   world\n\n\n\nsecond   para with enough characters to pass the minimum length\n").unwrap();
        let t = extract_text(p.to_str().unwrap()).unwrap();
        assert_eq!(t, "Hello world\n\nsecond para with enough characters to pass the minimum length");
        let found = scan(&[dir.display().to_string()]);
        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("note.txt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn default_folders_exist_or_are_empty() {
        for f in default_folders() {
            assert!(f.ends_with("Desktop") || f.ends_with("Documents") || f.ends_with("Downloads"));
        }
    }
}
