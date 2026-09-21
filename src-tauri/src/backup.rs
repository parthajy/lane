//! Encrypted backups: the whole vault as one file that only the user's
//! passphrase opens, written to a folder of their choice (a synced folder
//! such as iCloud Drive or Google Drive makes it follow them to a new Mac).
//!
//! Format: "RVLT1" + 16-byte salt + 24-byte nonce + XChaCha20-Poly1305
//! ciphertext of the plaintext SQLite file. Key = Argon2id(passphrase, salt).

use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 5] = b"RVLT1";
const KEEP: usize = 7;

fn derive(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let mut key = [0u8; 32];
    Argon2::default().hash_password_into(passphrase.as_bytes(), salt, &mut key).map_err(|e| e.to_string())?;
    Ok(key)
}

pub fn encrypt(plain: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
    let salt = crate::vault::random_bytes(16);
    let nonce = crate::vault::random_bytes(24);
    let key = derive(passphrase, &salt)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let ct = cipher.encrypt(XNonce::from_slice(&nonce), plain).map_err(|_| "encryption failed")?;
    let mut out = Vec::with_capacity(ct.len() + 45);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn decrypt(data: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
    if data.len() < 45 || &data[..5] != MAGIC {
        return Err("not a Lane backup file".into());
    }
    let key = derive(passphrase, &data[5..21])?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    cipher.decrypt(XNonce::from_slice(&data[21..45]), &data[45..]).map_err(|_| "wrong passphrase, or the file is damaged".into())
}

pub fn default_folder() -> PathBuf {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let icloud = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    if icloud.is_dir() { icloud.join("Lane") } else { home.join("Documents").join("Lane Backups") }
}

/// Write `<folder>/lane-YYYYMMDD-HHMM.rvault` from a plaintext export
/// and prune old ones. Returns the path.
pub fn write(conn: &rusqlite::Connection, folder: &Path, passphrase: &str, stamp: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let tmp = folder.join(".lane-export.tmp");
    crate::vault::export_plaintext(conn, &tmp)?;
    let plain = std::fs::read(&tmp).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    let out = folder.join(format!("lane-{stamp}.rvault"));
    std::fs::write(&out, encrypt(&plain, passphrase)?).map_err(|e| e.to_string())?;
    prune(folder);
    Ok(out)
}

fn prune(folder: &Path) {
    let Ok(rd) = std::fs::read_dir(folder) else { return };
    let mut files: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "rvault")).collect();
    files.sort();
    while files.len() > KEEP {
        let _ = std::fs::remove_file(files.remove(0));
    }
}

/// Decrypt a backup into a plaintext SQLite file at `dest`.
pub fn restore_to(file: &Path, passphrase: &str, dest: &Path) -> Result<(), String> {
    let data = std::fs::read(file).map_err(|e| e.to_string())?;
    let plain = decrypt(&data, passphrase)?;
    if !plain.starts_with(b"SQLite format 3\0") {
        return Err("the backup does not contain a database".into());
    }
    std::fs::write(dest, plain).map_err(|e| e.to_string())
}

pub fn latest(folder: &Path) -> Option<(PathBuf, i64)> {
    let rd = std::fs::read_dir(folder).ok()?;
    rd.filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rvault"))
        .filter_map(|p| {
            let m = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((p, m.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64))
        })
        .max_by_key(|(_, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_wrong_passphrase() {
        let data = b"SQLite format 3\0 pretend database contents".to_vec();
        let enc = encrypt(&data, "correct horse").unwrap();
        assert!(!enc.windows(8).any(|w| w == b"pretend "));
        assert_eq!(decrypt(&enc, "correct horse").unwrap(), data);
        assert!(decrypt(&enc, "wrong").is_err());
        assert!(decrypt(b"junk", "x").is_err());
    }

    #[test]
    fn write_restore_and_prune() {
        let dir = std::env::temp_dir().join(format!("rat-backup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES ('hello');").unwrap();
        for i in 0..9 {
            write(&conn, &dir, "pw", &format!("2026090{}-0000", i)).unwrap();
        }
        let n = std::fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "rvault")).count();
        assert_eq!(n, KEEP);
        let (last, _) = latest(&dir).unwrap();
        let dest = dir.join("restored.db");
        restore_to(&last, "pw", &dest).unwrap();
        let c = rusqlite::Connection::open(&dest).unwrap();
        let v: String = c.query_row("SELECT x FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "hello");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
