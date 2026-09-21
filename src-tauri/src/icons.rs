//! Icons for the source of a memory, found entirely on this Mac.
//!
//! Two places to look, in order: the website's icon in the browser's own
//! favicon cache (so a ChatGPT memory shows ChatGPT, not Chrome), then the
//! application's icon from its bundle. Nothing is fetched from the network —
//! that is the whole point of the app — so a site we have never visited in
//! Chrome simply falls back to the app, and then to a letter chip in the UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static C: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

fn mime_of(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" }
    else if bytes.starts_with(&[0xff, 0xd8]) { "image/jpeg" }
    else if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") { "image/svg+xml" }
    else { "image/png" }
}

/// Where an app with this display name lives, if it is installed.
fn app_bundle(name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() { return None; }
    let roots = [
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        home().join("Applications"),
    ];
    for r in roots.iter() {
        let direct = r.join(format!("{name}.app"));
        if direct.is_dir() { return Some(direct); }
    }
    // Chrome reports "Google Chrome", Code reports "Code", and so on: fall
    // back to a case-insensitive prefix match inside the same folders.
    let lower = name.to_lowercase();
    for r in roots.iter() {
        let Ok(rd) = std::fs::read_dir(r) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("app") { continue }
            let stem = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
            if stem == lower || stem.starts_with(&lower) || lower.starts_with(&stem) {
                return Some(p);
            }
        }
    }
    None
}

fn icns_of(bundle: &Path) -> Option<PathBuf> {
    let plist = bundle.join("Contents/Info.plist");
    let res = bundle.join("Contents/Resources");
    let out = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleIconFile", "raw", "-o", "-"])
        .arg(&plist)
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !name.is_empty() {
        let p = res.join(&name);
        if p.is_file() { return Some(p) }
        let p = res.join(format!("{name}.icns"));
        if p.is_file() { return Some(p) }
    }
    // Some bundles name the icon anything at all; take the first .icns there.
    std::fs::read_dir(&res).ok()?.flatten()
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|x| x.to_str()) == Some("icns"))
}

/// Render an .icns down to a small PNG with the system converter, cached on disk.
fn app_png(name: &str, data_dir: &Path) -> Option<Vec<u8>> {
    let slug: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let dir = data_dir.join("icons");
    let _ = std::fs::create_dir_all(&dir);
    let out = dir.join(format!("app-{slug}.png"));
    if let Ok(b) = std::fs::read(&out) { if !b.is_empty() { return Some(b) } }
    let bundle = app_bundle(name)?;
    let from_icns = icns_of(&bundle).map(|icns| {
        Command::new("/usr/bin/sips")
            .args(["-s", "format", "png", "--resampleHeightWidthMax", "64"])
            .arg(&icns).arg("--out").arg(&out)
            .output().map(|o| o.status.success()).unwrap_or(false)
    }).unwrap_or(false);
    if !from_icns {
        // Newer system apps keep their icon in an asset catalog, where there is
        // no .icns to convert. Quick Look renders the bundle's real icon.
        let tmp = std::env::temp_dir().join(format!("lane-ql-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let ok = Command::new("/usr/bin/qlmanage")
            .args(["-t", "-s", "128", "-o"])
            .arg(&tmp).arg(&bundle)
            .output().map(|o| o.status.success()).unwrap_or(false);
        let made = bundle.file_name().map(|n| tmp.join(format!("{}.png", n.to_string_lossy())));
        match (ok, made) {
            (true, Some(png)) if png.is_file() => { let _ = std::fs::copy(&png, &out); }
            _ => {}
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
    std::fs::read(&out).ok().filter(|b| !b.is_empty())
}

/// The site's own icon, read out of a browser's favicon cache on this disk.
fn site_png(domain: &str) -> Option<Vec<u8>> {
    let domain = domain.trim_start_matches("www.");
    if domain.is_empty() || !domain.contains('.') { return None }
    let dbs = [
        home().join("Library/Application Support/Google Chrome/Default/Favicons"),
        home().join("Library/Application Support/Google/Chrome/Default/Favicons"),
        home().join("Library/Application Support/BraveSoftware/Brave-Browser/Default/Favicons"),
        home().join("Library/Application Support/Microsoft Edge/Default/Favicons"),
        home().join("Library/Application Support/Arc/User Data/Default/Favicons"),
    ];
    for db in dbs.iter() {
        if !db.is_file() { continue }
        // The browser holds a lock on its own copy, so read a snapshot.
        let tmp = std::env::temp_dir().join(format!("lane-fav-{}.db", std::process::id()));
        if std::fs::copy(db, &tmp).is_err() { continue }
        let found = (|| -> Option<Vec<u8>> {
            let conn = rusqlite::Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
            let like = format!("%://{domain}/%");
            let like2 = format!("%.{domain}/%");
            conn.query_row(
                "SELECT b.image_data FROM favicon_bitmaps b \
                 JOIN icon_mapping m ON m.icon_id = b.icon_id \
                 WHERE m.page_url LIKE ?1 OR m.page_url LIKE ?2 \
                 ORDER BY b.width DESC LIMIT 1",
                rusqlite::params![like, like2],
                |r| r.get::<_, Vec<u8>>(0),
            ).ok().filter(|b| !b.is_empty())
        })();
        let _ = std::fs::remove_file(&tmp);
        if found.is_some() { return found }
    }
    None
}

/// A data URI for the source of a memory, or None when we have no icon for it.
pub fn source_icon(app: &str, domain: Option<&str>, data_dir: &Path) -> Option<String> {
    let key = format!("{app}|{}", domain.unwrap_or(""));
    if let Some(hit) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) { return hit }
    let bytes = domain.and_then(site_png).or_else(|| app_png(app, data_dir));
    let uri = bytes.map(|b| format!("data:{};base64,{}", mime_of(&b), b64(&b)));
    if let Ok(mut c) = cache().lock() { c.insert(key, uri.clone()); }
    uri
}
