//! Trial and licence, checked entirely on this Mac.
//!
//! Lane has no way to reach the network, so a licence cannot be "activated"
//! against a server and installs cannot be counted from here. Instead a key
//! is a short signed note: we sign it when someone buys, the app carries the
//! matching public key and checks the signature offline. The trial is a date
//! kept in the Keychain, so reinstalling does not hand out another seven
//! weeks.

use serde::{Deserialize, Serialize};

/// Two months, as promised on the site.
pub const TRIAL_DAYS: i64 = 60;
const TRIAL_SERVICE: &str = "so.lane.app.trial";
const LICENCE_SERVICE: &str = "so.lane.app.licence";
/// When the key now in use was first accepted on this Mac.
const SINCE_SERVICE: &str = "so.lane.app.licence.since";

/// How long a plan lasts. Keys are signed in batches long before anyone
/// buys one, so the date inside a key is when it was made, not when it was
/// sold: a monthly key minted in March would arrive at its buyer in June
/// already dead. The term therefore runs from the day the key is first
/// accepted here, which is also the day the buyer started getting Lane.
fn term_days(plan: &str) -> Option<i64> {
    match plan {
        "monthly" => Some(31),
        "yearly" => Some(366),
        _ => None, // lifetime, and anything we do not recognise, do not run out
    }
}

/// Days after a subscription ends before Lane stops. Long enough for a
/// renewal to arrive and be pasted in, short enough not to be a free month.
const GRACE_DAYS: i64 = 5;

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

/// A subscription's clock starts the first time its key is used here. The
/// answer is remembered per key, so pasting a different one starts again
/// and pasting the same one back does not.
fn started_on(key: &str) -> i64 {
    let stamp = fingerprint(key);
    if let Some(v) = crate::vault::keychain_get(SINCE_SERVICE) {
        if let Some((seen, at)) = v.split_once(':') {
            if seen == stamp {
                if let Ok(at) = at.parse::<i64>() {
                    if at > 0 {
                        return at;
                    }
                }
            }
        }
    }
    let now = now_ms();
    let _ = crate::vault::keychain_set(SINCE_SERVICE, &format!("{stamp}:{now}"));
    now
}

/// Enough of a key to tell it from another one, without keeping the key
/// twice over.
fn fingerprint(key: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(key.trim().as_bytes());
    hex::encode(&h.finalize()[..8])
}

/// Where this Mac stands right now.
pub fn status() -> Licence {
    if let Some(stored) = crate::vault::keychain_get(LICENCE_SERVICE) {
        if let Some((email, plan)) = verify(&stored) {
            let Some(days) = term_days(&plan) else {
                // Bought outright: nothing to count down.
                return Licence { state: "licensed".into(), days_left: 0, plan, email, trial_started_at: trial_start(), blocked: false };
            };
            let used = (now_ms() - started_on(&stored)) / 86_400_000;
            let left = days - used;
            if left > -GRACE_DAYS {
                return Licence { state: "licensed".into(), days_left: left.max(0), plan, email, trial_started_at: trial_start(), blocked: false };
            }
            // The term is over. Say which plan ran out, so the app can offer
            // the right thing rather than a bare "expired".
            return Licence { state: "expired".into(), days_left: 0, plan, email, trial_started_at: trial_start(), blocked: true };
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
    // Start the term now if this key has not been seen before.
    let _ = started_on(key);
    Ok(status())
}

/// Remove the key from this Mac (moving to another machine, or testing).
pub fn clear() -> Licence {
    let _ = crate::vault::keychain_set(LICENCE_SERVICE, "");
    status()
}

#[cfg(test)]
pub(crate) fn days_left_for(plan: &str, started_at: i64, now: i64) -> Option<i64> {
    let days = term_days(plan)?;
    Some(days - (now - started_at) / 86_400_000)
}

#[cfg(test)]
mod tests {
    const DAY: i64 = 86_400_000;

    #[test]
    fn a_subscription_runs_from_the_day_it_is_used() {
        let start = 1_700_000_000_000;
        assert_eq!(super::days_left_for("monthly", start, start), Some(31));
        assert_eq!(super::days_left_for("monthly", start, start + 30 * DAY), Some(1));
        assert_eq!(super::days_left_for("yearly", start, start + 100 * DAY), Some(266));
    }

    #[test]
    fn bought_outright_never_runs_out() {
        assert_eq!(super::days_left_for("lifetime", 0, 1_700_000_000_000), None);
        assert_eq!(super::term_days("lifetime"), None);
        // An unknown plan is treated as bought outright rather than locking
        // someone out of an app they paid for.
        assert_eq!(super::term_days("team"), None);
    }

    #[test]
    fn the_grace_is_days_not_weeks() {
        assert_eq!(super::GRACE_DAYS, 5);
        // A month gone by is over, but not yet shut off.
        let left = super::days_left_for("monthly", 0, 33 * DAY).unwrap();
        assert!(left < 0 && left > -super::GRACE_DAYS);
    }

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
