//! Encryption at rest. The database is a SQLCipher file; its key is a
//! random 256-bit value kept in the login Keychain, so a copied or stolen
//! database file is unreadable without the user's Mac login.
//!
//! The Keychain is used through the `security` tool so no other binary
//! needs Keychain entitlements; the item is readable only by that tool.

use std::path::Path;
use std::process::{Command, Stdio};

const ACCOUNT: &str = "reattend";
pub const VAULT_SERVICE: &str = "so.lane.app.vault";
pub const BACKUP_SERVICE: &str = "so.lane.app.backup";

pub fn random_bytes(n: usize) -> Vec<u8> {
    use std::io::Read;
    let mut buf = vec![0u8; n];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)).expect("urandom");
    buf
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn keychain_get(service: &str) -> Option<String> {
    let out = Command::new("security")
        .args(["find-generic-password", "-a", ACCOUNT, "-s", service, "-w"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

pub fn keychain_set(service: &str, value: &str) -> Result<(), String> {
    let status = Command::new("security")
        .args(["add-generic-password", "-a", ACCOUNT, "-s", service, "-w", value, "-U", "-T", "/usr/bin/security", "-l", "Lane"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err("could not store the key in the Keychain".into()) }
}

pub fn keychain_delete(service: &str) {
    let _ = Command::new("security").args(["delete-generic-password", "-a", ACCOUNT, "-s", service]).stdout(Stdio::null()).stderr(Stdio::null()).status();
}

/// The database key as SQLCipher's raw-key literal, creating it on first use.
pub fn vault_key() -> Result<String, String> {
    if let Some(k) = keychain_get(VAULT_SERVICE) {
        return Ok(k);
    }
    // Reattend-era key: adopt it so the existing vault still opens.
    if let Some(k) = keychain_get("com.reattend.mac.vault") {
        keychain_set(VAULT_SERVICE, &k)?;
        if let Some(b) = keychain_get("com.reattend.mac.backup") {
            let _ = keychain_set(BACKUP_SERVICE, &b);
        }
        log::info!("vault: adopted the existing database key");
        return Ok(k);
    }
    let key = hex(&random_bytes(32));
    keychain_set(VAULT_SERVICE, &key)?;
    log::info!("vault: created a new database key in the Keychain");
    Ok(key)
}

pub fn key_pragma(hex_key: &str) -> String {
    format!("\"x'{hex_key}'\"")
}

/// Is this file a plain SQLite database (unencrypted)?
pub fn is_plaintext_db(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 16];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).map(|_| &head == b"SQLite format 3\0").unwrap_or(false)
}

/// Encrypt an existing plaintext database in place (first launch after the
/// vault shipped). The plaintext file is overwritten securely afterwards.
pub fn encrypt_in_place(path: &Path, hex_key: &str) -> Result<(), String> {
    let enc = path.with_extension("db.enc");
    let _ = std::fs::remove_file(&enc);
    {
        let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch(&format!(
            "ATTACH DATABASE '{}' AS enc KEY {};
             SELECT sqlcipher_export('enc');
             DETACH DATABASE enc;",
            enc.display(),
            key_pragma(hex_key)
        ))
        .map_err(|e| format!("encrypting the database failed: {e}"))?;
    }
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
    }
    std::fs::rename(&enc, path).map_err(|e| e.to_string())?;
    log::info!("vault: database encrypted");
    Ok(())
}

/// Write an unencrypted copy (for backups and the training collector).
pub fn export_plaintext(conn: &rusqlite::Connection, dest: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(dest);
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS plain KEY '';
         SELECT sqlcipher_export('plain');
         DETACH DATABASE plain;",
        dest.display()
    ))
    .map_err(|e| format!("export failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_detection_and_encryption_roundtrip() {
        let dir = std::env::temp_dir().join(format!("rat-vault-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("m.db");
        {
            let c = rusqlite::Connection::open(&db).unwrap();
            c.execute_batch("CREATE TABLE t(x); INSERT INTO t VALUES ('secret tender text');").unwrap();
        }
        assert!(is_plaintext_db(&db));
        let key = hex(&random_bytes(32));
        encrypt_in_place(&db, &key).unwrap();
        assert!(!is_plaintext_db(&db));
        assert!(!std::fs::read(&db).unwrap().windows(6).any(|w| w == b"secret"), "no plaintext left in the file");
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch(&format!("PRAGMA key = {};", key_pragma(&key))).unwrap();
        let v: String = c.query_row("SELECT x FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "secret tender text");
        let plain = dir.join("plain.db");
        export_plaintext(&c, &plain).unwrap();
        assert!(is_plaintext_db(&plain));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
