//! Clipboard as a source, opt-in. Things the user copies are things they
//! cared about: a quote, a figure, an address, a paragraph. Every few
//! seconds the pasteboard is compared with what was last seen; new text
//! becomes a snapshot of a rolling "Clipboard" activity, which Rabbit turns
//! into memories like anything else. Same exclusions as screen capture;
//! anything that looks like a secret is never stored.

use crate::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

const POLL: Duration = Duration::from_secs(3);
/// Copies within this window join one "Clipboard" activity.
const SESSION_MS: i64 = 10 * 60_000;
const MIN_CHARS: usize = 20;
const MAX_CHARS: usize = 4_000;

/// Passwords, tokens and keys: one run of characters, mixed classes, no
/// spaces. Never stored, never logged.
pub fn looks_like_secret(text: &str) -> bool {
    let t = text.trim();
    if t.contains(char::is_whitespace) || t.chars().count() < 12 || t.chars().count() > 200 {
        return false;
    }
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let has_alpha = t.chars().any(|c| c.is_ascii_alphabetic());
    let has_symbol = t.chars().any(|c| !c.is_alphanumeric());
    let prefixed = ["sk-", "ghp_", "xox", "AKIA", "ntn_", "secret_", "eyJ"].iter().any(|p| t.starts_with(p));
    prefixed || (has_digit && has_alpha && (has_symbol || t.chars().count() >= 24))
}

/// Whether a copied text is worth keeping at all.
pub fn worth_keeping(text: &str) -> bool {
    let t = text.trim();
    t.chars().count() >= MIN_CHARS && !looks_like_secret(t) && t.chars().any(char::is_alphabetic)
}

pub fn spawn(state: Arc<AppState>) {
    std::thread::Builder::new()
        .name("clipboard".into())
        .spawn(move || {
            let mut board = match arboard::Clipboard::new() {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("clipboard: unavailable: {e}");
                    return;
                }
            };
            let mut last_hash: Option<String> = None;
            loop {
                std::thread::sleep(POLL);
                let enabled = crate::lock(&state.settings).clipboard_enabled;
                if !enabled || state.paused.load(Ordering::Relaxed) {
                    continue;
                }
                let Ok(text) = board.get_text() else { continue };
                let hash = crate::store::text_hash(&text);
                if last_hash.as_deref() == Some(hash.as_str()) {
                    continue;
                }
                let first_time = last_hash.is_none();
                last_hash = Some(hash);
                // What was on the clipboard before Lane started is not a new copy.
                if first_time || !worth_keeping(&text) {
                    continue;
                }
                let settings = crate::lock(&state.settings).clone();
                // The app in front is the likeliest source: honour its exclusion.
                if let Some(front) = crate::capture::platform::observe(false, false) {
                    if crate::privacy::is_excluded(&settings, &front.app_name, front.bundle_id.as_deref(), front.url.as_deref()) {
                        continue;
                    }
                }
                let text: String = text.trim().chars().take(MAX_CHARS).collect();
                let text = crate::privacy::redact_all(&text, settings.redact_contacts);
                let now = crate::capture::now_ms();
                match crate::lock(&state.store).clipboard_note(&text, now, SESSION_MS) {
                    Ok(_) => log::info!("clipboard: kept {} chars", text.chars().count()),
                    Err(e) => log::warn!("clipboard: {e}"),
                }
            }
        })
        .expect("spawn clipboard");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_and_scraps_are_not_kept() {
        assert!(looks_like_secret("sk-live-9f8a7b6c5d4e3f2a1b0c"));
        assert!(looks_like_secret("ghp_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6"));
        assert!(looks_like_secret("Tr0ub4dor&3xyz"));
        assert!(looks_like_secret("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
        assert!(!looks_like_secret("The budget is 35 lakh inclusive of GST"));
        assert!(!looks_like_secret("VatsalyaScheme"));
        assert!(!worth_keeping("short"));
        assert!(!worth_keeping("1234567890123456789012345"));
        assert!(worth_keeping("Please send the revised proposal by Friday, Sarah."));
    }
}
