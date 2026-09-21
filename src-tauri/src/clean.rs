//! Text cleanup: turn a raw accessibility dump into the content a person
//! would consider "what was on screen". Runs before storage; the raw text
//! is kept alongside so the rules can be re-run when they improve.

/// Lines this short that have been seen before from the same source (site
/// or app) are interface chrome: menus, labels, sidebars, tab names.
pub const BOILERPLATE_MAX_LEN: usize = 60;
/// Seen in at least this many earlier snapshots of the same source.
pub const BOILERPLATE_MIN_SEEN: u32 = 2;

/// Where a snapshot came from, for boilerplate statistics: the site for web
/// pages, the app otherwise.
pub fn source_key(app_name: &str, url: Option<&str>) -> String {
    if let Some(host) = url.and_then(host_of) {
        return host;
    }
    app_name.to_lowercase()
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?.trim_start_matches("www.");
    (!host.is_empty()).then(|| host.to_lowercase())
}

/// Apps whose windows carry no user content worth reading: file browsers,
/// settings, terminals and code editors (VS Code renders code on a canvas
/// the accessibility tree does not expose; only its sidebar leaks through).
pub const TITLE_ONLY_BUNDLES: &[&str] = &[
    "com.apple.finder",
    "com.apple.systempreferences",
    "com.apple.ActivityMonitor",
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.microsoft.VSCode",
    "com.todesktop.230313mzl4w4u92", // Cursor
    "com.apple.dt.Xcode",
    "com.jetbrains.intellij",
    "com.sublimetext.4",
];
pub const TITLE_ONLY_APPS: &[&str] = &[
    "Finder", "System Settings", "Activity Monitor", "Terminal", "iTerm2", "Code", "Visual Studio Code", "Cursor", "Xcode",
];

pub fn is_title_only(app_name: &str, bundle_id: Option<&str>) -> bool {
    bundle_id.is_some_and(|id| TITLE_ONLY_BUNDLES.contains(&id))
        || TITLE_ONLY_APPS.iter().any(|a| a.eq_ignore_ascii_case(app_name))
}

/// Browser- and editor-internal addresses carry no meaning for the user.
pub fn is_internal_url(url: &str) -> bool {
    const INTERNAL: &[&str] = &[
        "chrome://", "chrome-extension://", "edge://", "brave://", "arc://", "about:", "devtools://",
        "vscode-file://", "vscode-webview://", "javascript:",
    ];
    INTERNAL.iter().any(|p| url.starts_with(p))
}

fn is_symbolic(c: char) -> bool {
    // Private-use glyph fonts (icon fonts), variation selectors, box-drawing.
    matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{fe00}'..='\u{fe0f}' | '\u{2500}'..='\u{25ff}' | '\u{fffc}' | '\u{200b}'..='\u{200d}' | '\u{034f}' | '\u{feff}')
}

/// One raw line → cleaned line, or None if it carries nothing.
fn tidy_line(line: &str) -> Option<String> {
    let cleaned: String = line.chars().filter(|c| !is_symbolic(*c)).collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if !cleaned.chars().any(char::is_alphanumeric) {
        return None;
    }
    // Counters and fragments: "12", "of", "×". Real numbers in tables are
    // joined into their row at capture time and never stand alone.
    let n = cleaned.chars().count();
    if n <= 2 || (n <= 3 && cleaned.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    Some(cleaned)
}

/// Clean a raw snapshot. `seen_before(line)` says how many earlier snapshots
/// from the same source contained the line.
pub fn clean_text(raw: &str, mut seen_before: impl FnMut(&str) -> u32) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in raw.lines() {
        let Some(line) = tidy_line(line) else { continue };
        if line.chars().count() <= BOILERPLATE_MAX_LEN && seen_before(&line) >= BOILERPLATE_MIN_SEEN {
            continue;
        }
        if out.last().is_some_and(|prev| prev == &line) {
            continue;
        }
        out.push(line);
    }
    out.join("\n")
}

/// Distinct tidied lines of a raw snapshot, for updating source statistics.
pub fn distinct_lines(raw: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    raw.lines().filter_map(tidy_line).filter(|l| seen.insert(l.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_key_prefers_host() {
        assert_eq!(source_key("Google Chrome", Some("https://www.Mail.Google.com/mail/u/3/#inbox")), "mail.google.com");
        assert_eq!(source_key("Finder", None), "finder");
        assert_eq!(source_key("Code", Some("vscode-file://vscode-app/x")), "code");
    }

    #[test]
    fn title_only_apps() {
        assert!(is_title_only("Visual Studio Code", None));
        assert!(is_title_only("code", None));
        assert!(is_title_only("anything", Some("com.apple.finder")));
        assert!(!is_title_only("Notes", Some("com.apple.Notes")));
    }

    #[test]
    fn internal_urls_are_recognised() {
        assert!(is_internal_url("chrome://omnibox-popup.top-chrome/"));
        assert!(is_internal_url("vscode-file://vscode-app/private/var/x.html"));
        assert!(!is_internal_url("https://claude.ai/new"));
        assert!(!is_internal_url("file:///Users/p/report.pdf"));
    }

    #[test]
    fn drops_glyphs_and_empty_lines() {
        let raw = "\u{e5cd}\n□\n  Hello   world \n---\n42\nof\n2025\n\u{f101} Inbox";
        assert_eq!(clean_text(raw, |_| 0), "Hello world\n2025\nInbox");
    }

    #[test]
    fn drops_repeated_short_lines_but_keeps_long_and_new_ones() {
        let raw = "Inbox\nStarred\nBudget approved for the Vatsalya scheme communication proposal, 12 Oct\nInbox";
        let seen = |l: &str| if l == "Inbox" || l == "Starred" { 3 } else { 0 };
        assert_eq!(clean_text(raw, seen), "Budget approved for the Vatsalya scheme communication proposal, 12 Oct");
        // Same lines never seen before are kept.
        assert_eq!(clean_text("Inbox\nStarred", |_| 0), "Inbox\nStarred");
    }

    #[test]
    fn long_repeated_lines_are_content_not_boilerplate() {
        let long = "This paragraph is more than sixty characters long so it is real content even if repeated.";
        assert_eq!(clean_text(long, |_| 10), long);
    }

    #[test]
    fn distinct_lines_dedups() {
        assert_eq!(distinct_lines("abc\nbcd\nabc\n\u{e000}\n bcd "), vec!["abc", "bcd"]);
    }
}

/// What makes two activities "the same thing" for memory grouping: the page
/// without its query string for web content, the app plus window title
/// otherwise. Unread counters were already stripped from titles.
pub fn group_key(app_name: &str, window_title: &str, url: Option<&str>) -> String {
    if let Some(u) = url.filter(|u| u.starts_with("http") || u.starts_with("file:")) {
        let (base, frag) = u.split_once('#').map(|(b, f)| (b, Some(f))).unwrap_or((u, None));
        let base = base.split('?').next().unwrap_or(base).trim_end_matches('/');
        // Fragment routes (Gmail's #inbox/<thread>) identify content; drop
        // in-page anchors, which are short and have no slash.
        return match frag {
            Some(f) if f.contains('/') => format!("{}#{}", base.to_lowercase(), f.split('?').next().unwrap_or(f)),
            _ => base.to_lowercase(),
        };
    }
    let title = window_title.trim().to_lowercase();
    format!("{}|{}", app_name.to_lowercase(), title)
}

#[cfg(test)]
mod group_tests {
    use super::group_key;

    #[test]
    fn same_page_same_key() {
        let a = group_key("Google Chrome", "Claude - x", Some("https://claude.ai/chat/395d?x=1"));
        let b = group_key("Google Chrome", "Claude - y", Some("https://claude.ai/chat/395d/"));
        assert_eq!(a, b);
        assert_ne!(a, group_key("Google Chrome", "Claude", Some("https://claude.ai/chat/other")));
        // Gmail threads live in the fragment.
        assert_ne!(
            group_key("Google Chrome", "Mail", Some("https://mail.google.com/mail/u/3/#inbox/AAA")),
            group_key("Google Chrome", "Mail", Some("https://mail.google.com/mail/u/3/#inbox/BBB"))
        );
        assert_eq!(group_key("Google Chrome", "Docs", Some("https://x.com/page#section-2")), group_key("Google Chrome", "Docs", Some("https://x.com/page")));
        assert_eq!(group_key("WPS Office", "Vatsalya_IEC_Proposal.docx", None), "wps office|vatsalya_iec_proposal.docx");
    }
}

/// The app, or the site when the app is a browser: "Google Chrome" tells you
/// nothing, "github.com" tells you what the hour was for.
pub fn place_of(app: &str, url: Option<&str>) -> String {
    const BROWSERS: &[&str] = &["chrome", "safari", "firefox", "arc", "brave", "edge", "opera", "vivaldi", "orion"];
    let a = app.to_ascii_lowercase();
    if BROWSERS.iter().any(|b| a.contains(b)) {
        if let Some(host) = url.and_then(host_of) {
            return host;
        }
    }
    app.to_string()
}

/// What that place is for. Deliberately coarse: the point is to see the shape
/// of a month at a glance, not to file everything perfectly.
pub fn category_of(place: &str) -> &'static str {
    let p = place.to_ascii_lowercase();
    const SOCIAL: &[&str] = &["x.com", "twitter", "instagram", "facebook", "threads", "tiktok", "reddit", "linkedin", "mastodon", "bluesky", "pinterest", "snapchat"];
    const WATCH: &[&str] = &["youtube", "netflix", "primevideo", "hotstar", "spotify", "soundcloud", "twitch", "music", "vimeo", "podcast"];
    const TALK: &[&str] = &["mail", "gmail", "outlook", "slack", "whatsapp", "telegram", "messages", "discord", "zoom", "meet.google", "teams", "signal"];
    const BUILD: &[&str] = &["code", "xcode", "github", "gitlab", "terminal", "iterm", "warp", "stackoverflow", "localhost", "vercel", "netlify", "figma", "postman", "docker"];
    const WORK: &[&str] = &["docs.google", "sheets.google", "slides.google", "notion", "word", "excel", "powerpoint", "keynote", "pages", "numbers", "wpsoffice", "preview", "acrobat", "linear", "jira", "asana", "trello", "airtable"];
    const MONEY: &[&str] = &["dynadot", "godaddy", "namecheap", "stripe", "razorpay", "paypal", "bank", "zerodha", "invoice", "billing", "aws.amazon", "console.cloud"];
    const READ: &[&str] = &["medium", "substack", "news", "bbc", "nytimes", "wikipedia", "arxiv", "blog", "hacker"];
    const SHOP: &[&str] = &["amazon", "flipkart", "myntra", "ebay", "etsy", "swiggy", "zomato", "uber"];
    const ASSIST: &[&str] = &["chatgpt", "claude", "gemini", "perplexity", "copilot", "openai"];
    const BROWSERS: &[&str] = &["chrome", "safari", "firefox", "arc", "brave", "edge", "opera", "vivaldi", "orion"];
    let has = |list: &[&str]| list.iter().any(|w| p.contains(w));
    // A browser with no address to go on is just browsing.
    if BROWSERS.iter().any(|b| p.contains(b)) && !p.contains('.') { return "Browsing" }
    if has(ASSIST) { "Assistants" }
    else if has(SOCIAL) { "Social" }
    else if has(WATCH) { "Watching and listening" }
    else if has(TALK) { "Talking to people" }
    else if has(BUILD) { "Building" }
    else if has(WORK) { "Documents and planning" }
    else if has(MONEY) { "Money and admin" }
    else if has(SHOP) { "Shopping and errands" }
    else if has(READ) { "Reading" }
    else { "Everything else" }
}
