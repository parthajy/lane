//! Privacy rules applied before anything touches disk.

use crate::store::Settings;

/// Always skipped, not user-editable: system prompts that can show
/// passwords or biometrics, lock/screensaver, system overlays, and
/// password managers.
pub const SYSTEM_EXCLUDED_BUNDLES: &[&str] = &[
    // Lane itself (installed or another copy running).
    "so.lane.app",
    "com.reattend.mac",
    "com.apple.SecurityAgent",
    "com.apple.LocalAuthentication.UIAgent",
    "com.apple.loginwindow",
    "com.apple.ScreenSaver.Engine",
    "com.apple.screencaptureui",
    "com.apple.accessibility.universalAccessAuthWarn",
    "com.apple.UserNotificationCenter",
    "com.apple.CoreServicesUIAgent",
    "com.apple.dock",
    "com.apple.controlcenter",
    "com.apple.notificationcenterui",
    "com.apple.Spotlight",
    "com.apple.systemuiserver",
    "com.apple.WindowManager",
    "com.apple.keychainaccess",
    "com.apple.Passwords",
    "com.1password.1password",
    "com.agilebits.onepassword7",
    "com.bitwarden.desktop",
    "com.dashlane.dashlanephonefinal",
    "com.lastpass.LastPass",
    "org.keepassxc.keepassxc",
];

/// Name fallback for when the bundle id can't be read.
pub const SYSTEM_EXCLUDED_APPS: &[&str] = &[
    "Lane",
    "Reattend",
    "rat-mac",
    "SecurityAgent",
    "coreautha",
    "loginwindow",
    "ScreenSaverEngine",
    "screencaptureui",
    "Screenshot",
    "universalAccessAuthWarn",
    "UserNotificationCenter",
    "CoreServicesUIAgent",
    "Dock",
    "Control Center",
    "Notification Center",
    "Spotlight",
    "SystemUIServer",
    "WindowManager",
    "Keychain Access",
    "Passwords",
    "1Password",
    "1Password 7",
    "Bitwarden",
    "Dashlane",
    "LastPass",
    "KeePassXC",
];

pub const MESSAGING_APPS: &[&str] = &["Messages", "WhatsApp", "Telegram", "Signal", "Messenger", "Discord"];
pub const MESSAGING_URLS: &[&str] = &[
    "web.whatsapp.com",
    "web.telegram.org",
    "messenger.com",
    "facebook.com/messages",
    "instagram.com/direct",
    "discord.com/channels",
    "messages.google.com",
];
pub const EMAIL_APPS: &[&str] = &["Mail", "Microsoft Outlook", "Spark", "Airmail", "Mimestream"];
pub const EMAIL_URLS: &[&str] = &[
    "mail.google.com",
    "outlook.live.com",
    "outlook.office.com",
    "outlook.office365.com",
    "mail.yahoo.com",
    "mail.proton.me",
];

fn eq_any(list: impl IntoIterator<Item = impl AsRef<str>>, value: &str) -> bool {
    list.into_iter().any(|e| {
        let e = e.as_ref().trim();
        !e.is_empty() && e.eq_ignore_ascii_case(value)
    })
}

fn url_matches(list: impl IntoIterator<Item = impl AsRef<str>>, url: &str) -> bool {
    let url = url.to_lowercase();
    list.into_iter().any(|p| {
        let p = p.as_ref().trim().to_lowercase();
        !p.is_empty() && url.contains(&p)
    })
}

pub fn is_excluded(settings: &Settings, app_name: &str, bundle_id: Option<&str>, url: Option<&str>) -> bool {
    if bundle_id.is_some_and(|id| SYSTEM_EXCLUDED_BUNDLES.iter().any(|b| b.eq_ignore_ascii_case(id)))
        || eq_any(SYSTEM_EXCLUDED_APPS, app_name)
        || eq_any(&settings.excluded_apps, app_name)
        || (settings.exclude_messaging && eq_any(MESSAGING_APPS, app_name))
        || (settings.exclude_email && eq_any(EMAIL_APPS, app_name))
    {
        return true;
    }
    let Some(url) = url else { return false };
    url_matches(&settings.excluded_url_patterns, url)
        || (settings.exclude_messaging && url_matches(MESSAGING_URLS, url))
        || (settings.exclude_email && url_matches(EMAIL_URLS, url))
}

/// Everything that must never reach disk, plus contact details when the
/// setting is on (default).
pub fn redact_all(text: &str, contacts: bool) -> String {
    let text = redact(text);
    if contacts { redact_contacts(&text) } else { text }
}

/// Replace email addresses and phone numbers with markers. Phone numbers:
/// an optional +country code, then 10-12 digits with spaces, dashes or
/// brackets between them. Shorter runs (years, amounts, IDs) are kept.
pub fn redact_contacts(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut rest = line;
        while let Some((start, end)) = find_email(rest) {
            out.push_str(&rest[..start]);
            out.push_str("[email]");
            rest = &rest[end..];
        }
        out.push_str(rest);
    }
    redact_phones(&out)
}

fn find_email(s: &str) -> Option<(usize, usize)> {
    let at = s.find('@')?;
    let bytes = s.as_bytes();
    let local_ok = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'%' | b'+' | b'-');
    let mut start = at;
    while start > 0 && local_ok(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = at + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'.' | b'-')) {
        end += 1;
    }
    let domain = &s[at + 1..end];
    if start == at || !domain.contains('.') || domain.ends_with('.') {
        // Not an address ("@channel", "user@localhost"): skip past this '@'.
        return find_email(&s[at + 1..]).map(|(a, b)| (a + at + 1, b + at + 1));
    }
    Some((start, end))
}

fn redact_phones(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let is_sep = |c: char| matches!(c, ' ' | '-' | '(' | ')');
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next_digit = i + 1 < chars.len() && chars[i + 1].is_ascii_digit();
        let starts = c.is_ascii_digit() || ((c == '+' || c == '(') && next_digit);
        // Only at a token boundary: not inside words or longer numbers.
        let boundary = i == 0 || !(chars[i - 1].is_ascii_alphanumeric() || matches!(chars[i - 1], '.' | ',' | '+'));
        if starts && boundary {
            let mut j = if c.is_ascii_digit() { i } else { i + 1 };
            let mut digits = 0;
            let mut end = i;
            loop {
                if j < chars.len() && chars[j].is_ascii_digit() {
                    digits += 1;
                    end = j + 1;
                    j += 1;
                    continue;
                }
                // Up to two separators, but only if digits follow.
                let mut k = j;
                while k < chars.len() && k - j < 2 && is_sep(chars[k]) {
                    k += 1;
                }
                if k > j && k < chars.len() && chars[k].is_ascii_digit() {
                    j = k;
                    continue;
                }
                break;
            }
            let followed_by_word = end < chars.len()
                && (chars[end].is_ascii_alphanumeric() || (chars[end] == '.' && end + 1 < chars.len() && chars[end + 1].is_ascii_digit()));
            if (10..=13).contains(&digits) && !followed_by_word {
                out.push_str("[phone]");
                i = end;
                continue;
            }
            let stop = end.max(i + 1);
            out.extend(&chars[i..stop]);
            i = stop;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Replace card-number-like digit runs (13-19 digits, Luhn-valid, spaces or
/// dashes allowed) with a marker.
pub fn redact(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            // Greedily take digits with single space/dash separators.
            let mut j = i;
            let mut digits = String::new();
            let mut end = i;
            while j < chars.len() {
                if chars[j].is_ascii_digit() {
                    digits.push(chars[j]);
                    end = j + 1;
                    j += 1;
                } else if (chars[j] == ' ' || chars[j] == '-')
                    && j + 1 < chars.len()
                    && chars[j + 1].is_ascii_digit()
                {
                    j += 1;
                } else {
                    break;
                }
            }
            if (13..=19).contains(&digits.len()) && luhn_valid(&digits) {
                out.push_str("[redacted card]");
            } else {
                out.extend(&chars[i..end]);
            }
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn luhn_valid(digits: &str) -> bool {
    let mut sum = 0;
    for (idx, c) in digits.chars().rev().enumerate() {
        let mut d = c.to_digit(10).unwrap_or(0);
        if idx % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompts_and_password_managers_always_excluded() {
        let s = Settings { excluded_apps: vec![], exclude_messaging: false, exclude_email: false, ..Settings::default() };
        assert!(is_excluded(&s, "anything", Some("com.apple.SecurityAgent"), None));
        assert!(is_excluded(&s, "screencaptureui", None, None));
        assert!(is_excluded(&s, "1password", None, None));
        assert!(is_excluded(&s, "rat-mac", None, None), "dev builds of Lane");
        assert!(is_excluded(&s, "Anything", Some("com.reattend.mac"), None));
        assert!(!is_excluded(&s, "Google Chrome", Some("com.google.Chrome"), Some("https://example.com")));
    }

    #[test]
    fn user_app_and_url_rules() {
        let s = Settings {
            excluded_apps: vec!["Zoom".into()],
            excluded_url_patterns: vec!["mybank.com".into(), " ".into()],
            ..Settings::default()
        };
        assert!(is_excluded(&s, "zoom", None, None));
        assert!(is_excluded(&s, "Safari", None, Some("https://secure.MyBank.com/login")));
        assert!(!is_excluded(&s, "Safari", None, Some("https://news.ycombinator.com")));
    }

    #[test]
    fn messaging_and_email_presets() {
        let on = Settings { exclude_messaging: true, exclude_email: true, ..Settings::default() };
        let off = Settings { exclude_messaging: false, exclude_email: false, ..Settings::default() };
        let wa = Some("https://web.whatsapp.com/");
        assert!(is_excluded(&on, "Google Chrome", None, wa));
        assert!(!is_excluded(&off, "Google Chrome", None, wa));
        assert!(is_excluded(&on, "Messages", None, None));
        assert!(is_excluded(&on, "Safari", None, Some("https://mail.google.com/mail/u/0")));
        assert!(!is_excluded(&off, "Mail", None, None));
    }

    #[test]
    fn redacts_emails_and_phones_but_not_ordinary_numbers() {
        assert_eq!(redact_contacts("mail parthajy@gmail.com or pb@lane.so now"), "mail [email] or [email] now");
        assert_eq!(redact_contacts("Phone 7002808244"), "Phone [phone]");
        assert_eq!(redact_contacts("call +91 70028 08244 or (415) 555-2671 today"), "call [phone] or [phone] today");
        // Years, amounts, IDs, CINs and short numbers stay.
        assert_eq!(redact_contacts("in 2026, ₹35,00,000 and 300+ startups, CIN U62013AS2026PTC030258"), "in 2026, ₹35,00,000 and 300+ startups, CIN U62013AS2026PTC030258");
        assert_eq!(redact_contacts("Report-ID: 16316729958198192805"), "Report-ID: 16316729958198192805");
        assert_eq!(redact_contacts("@channel see user@localhost"), "@channel see user@localhost");
        assert_eq!(redact_all("card 4242 4242 4242 4242, mail a@b.co", false), "card [redacted card], mail a@b.co");
        assert_eq!(redact_all("card 4242 4242 4242 4242, mail a@b.co", true), "card [redacted card], mail [email]");
    }

    #[test]
    fn redacts_valid_card_numbers_only() {
        assert_eq!(redact("card 4242 4242 4242 4242 ok"), "card [redacted card] ok");
        assert_eq!(redact("4111-1111-1111-1111"), "[redacted card]");
        // Not Luhn-valid, and short numbers, stay intact.
        assert_eq!(redact("order 1234 5678 9012 3456"), "order 1234 5678 9012 3456");
        assert_eq!(redact("deadline 12 October 2026"), "deadline 12 October 2026");
    }
}
