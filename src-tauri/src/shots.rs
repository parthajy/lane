//! Screenshots for memories: a small JPEG of the front window, taken by the
//! `lane-shot` helper when a new text snapshot is stored (opt-in, needs
//! Screen Recording). Kept beside the database, pruned with raw text.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static HELPER: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn init(resource_dir: Option<&Path>) {
    let mut candidates = Vec::new();
    if let Some(r) = resource_dir {
        candidates.push(r.join("audio").join("lane-shot"));
    }
    candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/audio/lane-shot")));
    let _ = HELPER.set(candidates.into_iter().find(|p| p.is_file()));
}

fn helper() -> Option<&'static Path> {
    HELPER.get().and_then(|h| h.as_deref())
}

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("shots")
}

/// Screen Recording granted to this app.
pub fn granted() -> bool {
    helper().map_or(false, |h| std::process::Command::new(h).arg("--check").status().map_or(false, |s| s.success()))
}

/// Ask macOS; the prompt appears once and the grant needs an app restart.
pub fn request() -> bool {
    helper().map_or(false, |h| std::process::Command::new(h).arg("--request").status().map_or(false, |s| s.success()))
}

/// Take the picture; returns the file path. The helper reads the front
/// window itself, so this is called right after the text was read.
pub fn take(data_dir: &Path, activity_id: i64, now: i64) -> Option<String> {
    let h = helper()?;
    let d = dir(data_dir);
    std::fs::create_dir_all(&d).ok()?;
    let path = d.join(format!("{activity_id}-{now}.jpg"));
    let out = std::process::Command::new(h).arg(&path).arg("1280").output().ok()?;
    if !out.status.success() || !path.is_file() {
        let why = String::from_utf8_lossy(&out.stdout);
        if why.contains("no-permission") {
            log::info!("shots: Screen Recording not granted");
        }
        return None;
    }
    Some(path.display().to_string())
}

pub fn remove_all(paths: &[String]) {
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
}

/// A snapshot's picture as a data URL for the UI; only files under the
/// shots folder are served.
pub fn data_url(data_dir: &Path, path: &str) -> Result<String, String> {
    let p = PathBuf::from(path);
    let root = dir(data_dir).canonicalize().map_err(|e| e.to_string())?;
    let full = p.canonicalize().map_err(|_| "no such picture".to_string())?;
    if !full.starts_with(&root) {
        return Err("not a Lane picture".into());
    }
    let bytes = std::fs::read(&full).map_err(|e| e.to_string())?;
    Ok(format!("data:image/jpeg;base64,{}", base64_encode(&bytes)))
}

fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(super::base64_encode(b"Man"), "TWFu");
        assert_eq!(super::base64_encode(b"Ma"), "TWE=");
        assert_eq!(super::base64_encode(b"M"), "TQ==");
    }
}
