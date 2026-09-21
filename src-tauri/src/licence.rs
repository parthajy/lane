//! Trial and licence, checked entirely on this Mac.
//!
//! Lane has no way to reach the network, so a licence cannot be "activated"
//! against a server and installs cannot be counted from here. Instead a key
//! is a short signed note: we sign it when someone buys, the app carries the
//! matching public key and checks the signature offline. The trial is a date
//! kept in the Keychain, so reinstalling does not hand out another seven
//! weeks.

use serde::{Deserialize, Serialize};

/// Seven weeks, as promised on the site.
pub const TRIAL_DAYS: i64 = 49;
const TRIAL_SERVICE: &str = "so.lane.app.trial";
const LICENCE_SERVICE: &str = "so.lane.app.licence";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Licence {
    /// "trial", "licensed" or "expired".
    pub state: String,
    pub days_left: i64,
    /// "monthly", "yearly", "lifetime" or "" while on trial.
    pub plan: String,
    pub email: String,
    pub trial_started_at: i64,
    /// True once the trial has run out and no key has been entered.
    pub blocked: bool,
}

fn now_ms() -> i64 {
    crate::capture::now_ms()
}

/// When this Mac first ran Lane. Written once, kept in the Keychain so it
/// survives deleting the app and its data folder.
fn trial_start() -> i64 {
    if let Some(v) = crate::vault::keychain_get(TRIAL_SERVICE).and_then(|s| s.trim().parse::<i64>().ok()) {
        if v > 0 {
            return v;
        }
    }
    let now = now_ms();
    let _ = crate::vault::keychain_set(TRIAL_SERVICE, &now.to_string());
    now
}

/// `lane1|<email>|<plan>|<issued ms>` signed with the same key that signs
/// updates. Stored as `<payload>::<signature>`, where the signature is the
/// minisign signature file in base64, so the whole key is one line.
fn verify(stored: &str) -> Option<(String, String)> {
    let (payload, sig) = stored.split_once("::")?;
    // Two keys are trusted: the licence key, which signs what people buy,
    // and the key that signs updates, which issued the first keys by hand.
    // A licence key living apart from the update key means one can be
    // replaced without stranding the other.
    let keys = [option_env!("LANE_LICENCE_PUBKEY").unwrap_or(PUBKEY_B64), UPDATE_PUBKEY_B64];
    // The signature travels as one pasteable line: base64 of the whole
    // minisign signature file. Older hand-made keys carried the file itself.
    let sig = sig.trim();
    let text = b64_decode(sig)
        .and_then(|b| String::from_utf8(b).ok())
        .filter(|t| t.contains("trusted comment"))
        .unwrap_or_else(|| sig.to_string());
    let signature = minisign_verify::Signature::decode(&text).ok()?;
    let ok = keys.iter().any(|k| {
        b64_decode(k)
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|t| t.lines().last().map(|l| l.trim().to_string()))
            .and_then(|line| minisign_verify::PublicKey::from_base64(&line).ok())
            .map(|pk| pk.verify(payload.as_bytes(), &signature, false).is_ok())
            .unwrap_or(false)
    });
    if !ok {
        return None;
    }
    let mut parts = payload.split('|');
    if parts.next()? != "lane1" {
        return None;
    }
    let email = parts.next()?.to_string();
    let plan = parts.next()?.to_string();
    Some((email, plan))
}

/// The public half of the licence key. Its secret lives in
/// ~/.tauri/lane-licence.json and signs what people buy
/// (`node scripts/licence.mjs`).
const PUBKEY_B64: &str = include_str!("licence_pubkey.txt");

/// The key that signs updates, still trusted so the keys issued by hand
/// before there was a licence key keep working.
const UPDATE_PUBKEY_B64: &str = include_str!("update_pubkey.txt");

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let clean: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut acc = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            acc |= val(*c)? << (18 - 6 * i);
        }
        out.push((acc >> 16) as u8);
        if chunk.len() > 2 { out.push((acc >> 8) as u8); }
        if chunk.len() > 3 { out.push(acc as u8); }
    }
    Some(out)
}

/// Where this Mac stands right now.
pub fn status() -> Licence {
    if let Some(stored) = crate::vault::keychain_get(LICENCE_SERVICE) {
        if let Some((email, plan)) = verify(&stored) {
            return Licence { state: "licensed".into(), days_left: 0, plan, email, trial_started_at: trial_start(), blocked: false };
        }
    }
    let started = trial_start();
    let used = (now_ms() - started) / 86_400_000;
    let left = TRIAL_DAYS - used;
    if left > 0 {
        Licence { state: "trial".into(), days_left: left, plan: String::new(), email: String::new(), trial_started_at: started, blocked: false }
    } else {
        Licence { state: "expired".into(), days_left: 0, plan: String::new(), email: String::new(), trial_started_at: started, blocked: true }
    }
}

/// Paste a key from the receipt. Returns the new status, or says why not.
pub fn apply(key: &str) -> Result<Licence, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Paste the key from your receipt.".into());
    }
    if verify(key).is_none() {
        return Err("That key is not valid for this version of Lane. Check you copied all of it.".into());
    }
    crate::vault::keychain_set(LICENCE_SERVICE, key)?;
    Ok(status())
}

/// Remove the key from this Mac (moving to another machine, or testing).
pub fn clear() -> Licence {
    let _ = crate::vault::keychain_set(LICENCE_SERVICE, "");
    status()
}

#[cfg(test)]
mod tests {
    /// Round trip: a key made by scripts/issue-licence.sh must verify here.
    /// Run with LANE_TEST_KEY="$(scripts/issue-licence.sh a@b.com lifetime)".
    #[test]
    fn issued_key_verifies() {
        let Ok(key) = std::env::var("LANE_TEST_KEY") else { return };
        let got = super::verify(&key).expect("the issued key should verify");
        assert_eq!(got.1, "lifetime");
        assert!(!got.0.is_empty());

        // A tampered payload must not.
        let bad = key.replacen("lifetime", "monthly", 1);
        assert!(super::verify(&bad).is_none(), "a edited key must be refused");
    }

    /// The whole stored path: apply a key, read it back, then clear it.
    /// Touches the real Keychain, so it is opt in.
    #[test]
    #[ignore]
    fn apply_and_clear() {
        let key = std::env::var("LANE_TEST_KEY").expect("LANE_TEST_KEY");
        let before = super::status();
        let applied = super::apply(&key).expect("apply should succeed");
        assert_eq!(applied.state, "licensed");
        assert_eq!(applied.plan, "lifetime");
        assert!(!applied.blocked);
        assert!(super::apply("not a key").is_err());
        let after = super::clear();
        assert_eq!(after.state, before.state, "clearing must put it back on trial");
        assert!(super::status().plan.is_empty());
    }
}
