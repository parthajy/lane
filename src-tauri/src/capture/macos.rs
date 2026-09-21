//! macOS capture via the Accessibility (AX) API and CoreGraphics.
//!
//! Without Accessibility permission we still know which app is in front
//! (CGWindowList needs no permission) but get no window title or text.
//! No screenshots, no keystrokes: only text the app already exposes to
//! assistive technology.

#![allow(non_upper_case_globals, clippy::upper_case_acronyms)]

use super::Observation;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use core_foundation::url::CFURL;
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{CFGetTypeID, CFRelease, CFRetain, CFTypeID, CFTypeRef};
use core_foundation_sys::bundle::{CFBundleCreate, CFBundleGetIdentifier};
use core_foundation_sys::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation_sys::number::{kCFNumberSInt64Type, CFNumberGetValue, CFNumberRef};
use core_foundation_sys::string::{CFStringGetTypeID, CFStringRef};
use core_foundation_sys::url::{CFURLGetString, CFURLGetTypeID, CFURLRef};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::Mutex;
use std::time::{Duration, Instant};

type AXUIElementRef = CFTypeRef;
type AXError = i32;
const kAXErrorSuccess: AXError = 0;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: *mut CFTypeRef) -> AXError;
    fn AXUIElementSetAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: CFTypeRef) -> AXError;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_seconds: f32) -> AXError;
    fn AXUIElementGetTypeID() -> CFTypeID;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CFArrayRef;
    fn CGEventSourceSecondsSinceLastEventType(source_state: i32, event_type: u32) -> f64;
}

const kCGWindowListOptionOnScreenOnly: u32 = 1 << 0;
const kCGWindowListExcludeDesktopElements: u32 = 1 << 4;
const kCGEventSourceStateCombinedSessionState: i32 = 0;
const kCGAnyInputEventType: u32 = u32::MAX;

/// Limits for one walk of a window's accessibility tree. Keeps a poll cheap
/// even on huge documents or apps that answer AX queries slowly.
const MAX_NODES: usize = 4_000;
const MAX_DEPTH: usize = 60;
const MAX_CHARS: usize = 30_000;
const WALK_BUDGET: Duration = Duration::from_millis(400);

/// Chromium-based browsers only build their web accessibility tree when an
/// assistive technology asks. Electron apps are deliberately not asked:
/// apps like VS Code treat it as a screen reader turning on and change
/// their behaviour (VS Code prompts to enable screen-reader mode).
const CHROMIUM_BUNDLE_PREFIXES: &[&str] = &[
    "com.google.Chrome",
    "org.chromium.Chromium",
    "com.brave.Browser",
    "com.microsoft.edgemac",
    "company.thebrowser.Browser",
    "com.vivaldi.Vivaldi",
    "com.operasoftware.Opera",
];

/// WebKit browsers expose page content fully only to a client that asks for
/// the "enhanced user interface" (what VoiceOver sets). Without it Safari
/// shows titles and an empty web area.
const WEBKIT_BUNDLES: &[&str] = &["com.apple.Safari", "com.apple.SafariTechnologyPreview", "com.kagi.kagimacOS"];

/// Per-process facts that never change while the process lives.
#[derive(Clone)]
struct AppInfo {
    path: Option<String>,
    bundle_id: Option<String>,
    web_tree: bool,
    webkit: bool,
}

static APPS: Mutex<Option<HashMap<i32, AppInfo>>> = Mutex::new(None);
static WEB_TREE_ENABLED: Mutex<Option<HashSet<i32>>> = Mutex::new(None);

// ── Owned CF reference ───────────────────────────────────────────────────

struct Cf(CFTypeRef);

impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}

impl Cf {
    fn type_id(&self) -> CFTypeID {
        unsafe { CFGetTypeID(self.0) }
    }

    fn as_string(&self) -> Option<String> {
        unsafe {
            if self.type_id() == CFStringGetTypeID() {
                Some(CFString::wrap_under_get_rule(self.0 as CFStringRef).to_string())
            } else if self.type_id() == CFURLGetTypeID() {
                let s = CFURLGetString(self.0 as CFURLRef);
                (!s.is_null()).then(|| CFString::wrap_under_get_rule(s).to_string())
            } else {
                None
            }
        }
    }
}

struct Element(Cf);

impl Element {
    fn attr(&self, name: &str) -> Option<Cf> {
        let key = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(self.0 .0, key.as_concrete_TypeRef(), &mut value) };
        (err == kAXErrorSuccess && !value.is_null()).then(|| Cf(value))
    }

    fn string(&self, name: &str) -> Option<String> {
        self.attr(name).and_then(|v| v.as_string()).filter(|s| !s.trim().is_empty())
    }

    fn element(&self, name: &str) -> Option<Element> {
        let v = self.attr(name)?;
        (v.type_id() == unsafe { AXUIElementGetTypeID() }).then(|| Element(v))
    }

    /// Element `i` of an array attribute such as AXWindows.
    #[allow(dead_code)]
    fn element_at(&self, name: &str, i: isize) -> Option<Element> {
        let arr = self.attr(name)?;
        unsafe {
            if arr.type_id() != CFArrayGetTypeID() || CFArrayGetCount(arr.0 as CFArrayRef) <= i {
                return None;
            }
            let p = CFArrayGetValueAtIndex(arr.0 as CFArrayRef, i);
            (!p.is_null() && CFGetTypeID(p) == AXUIElementGetTypeID()).then(|| Element(Cf(CFRetain(p))))
        }
    }

    fn children(&self) -> Vec<Element> {
        let Some(arr) = self.attr("AXChildren") else { return Vec::new() };
        unsafe {
            if arr.type_id() != CFArrayGetTypeID() {
                return Vec::new();
            }
            let n = CFArrayGetCount(arr.0 as CFArrayRef).min(MAX_NODES as isize);
            (0..n)
                .map(|i| CFArrayGetValueAtIndex(arr.0 as CFArrayRef, i))
                .filter(|p| !p.is_null() && CFGetTypeID(*p) == AXUIElementGetTypeID())
                .map(|p| Element(Cf(CFRetain(p))))
                .collect()
        }
    }

    fn pid(&self) -> Option<i32> {
        let mut pid = 0;
        (unsafe { AXUIElementGetPid(self.0 .0, &mut pid) } == kAXErrorSuccess).then_some(pid)
    }

    fn set_bool(&self, name: &str, value: bool) {
        let key = CFString::new(name);
        let v = if value { CFBoolean::true_value() } else { CFBoolean::false_value() };
        unsafe {
            AXUIElementSetAttributeValue(self.0 .0, key.as_concrete_TypeRef(), v.as_CFTypeRef());
        }
    }
}

// ── Public API ───────────────────────────────────────────────────────────

pub fn is_trusted(prompt: bool) -> bool {
    unsafe {
        if !prompt {
            return AXIsProcessTrusted();
        }
        let options = CFDictionary::from_CFType_pairs(&[(
            CFString::new("AXTrustedCheckOptionPrompt").as_CFType(),
            CFBoolean::true_value().as_CFType(),
        )]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
    }
}

/// True when accessibility calls actually work, not just when the trust
/// flag says so. After a permission change macOS can report trusted while
/// calls still fail until the app restarts.
pub fn accessibility_works() -> bool {
    let Some((pid, _)) = frontmost_window_owner() else { return false };
    let app = app_element(pid);
    app.attr("AXRole").is_some()
}

/// Seconds since the last keyboard/mouse/trackpad event. Needs no permission.
pub fn idle_seconds() -> f64 {
    unsafe { CGEventSourceSecondsSinceLastEventType(kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType) }
}

/// What is in front of the user, or None when nothing should be recorded
/// (including when Lane itself is in front). With `with_text` the
/// window's accessibility tree is walked for on-screen text; without it
/// only app, title and URL are read, which is much cheaper.
pub fn observe(read_web_text: bool, with_text: bool) -> Option<Observation> {
    let (pid, owner_name) = focused_pid().or_else(frontmost_window_owner)?;
    if pid == std::process::id() as i32 {
        return None;
    }
    observe_pid(pid, owner_name, read_web_text, with_text)
}

/// The topmost window that is not Lane's own, read on purpose.
///
/// The background pass only ever looks at what has focus, and it refuses to
/// look at Lane. But "Capture my screen" is pressed in Lane's menu bar, which
/// takes focus, so it needs to reach past us to whatever is behind.
pub fn observe_front_other(read_web_text: bool, with_text: bool) -> Option<Observation> {
    let me = std::process::id() as i32;
    let (pid, owner_name) = focused_pid()
        .filter(|(p, _)| *p != me)
        .or_else(|| frontmost_other_owner(me))?;
    observe_pid(pid, owner_name, read_web_text, with_text)
}

fn observe_pid(pid: i32, owner_name: Option<String>, read_web_text: bool, with_text: bool) -> Option<Observation> {
    let info = app_info(pid);
    // Bundle name first: stable across capture modes ("Google Chrome", not
    // sometimes "Chrome"; "Visual Studio Code", not sometimes "Code").
    let fallback_name = || {
        info.path
            .as_deref()
            .filter(|p| p.ends_with(".app") || p.ends_with(".app.bundle"))
            .and_then(bundle_stem)
            .or_else(|| owner_name.clone())
            .unwrap_or_else(|| format!("pid {pid}"))
    };
    let base = Observation {
        app_name: fallback_name(),
        app_path: info.path.clone(),
        bundle_id: info.bundle_id.clone(),
        ..Default::default()
    };
    if !is_trusted(false) {
        return Some(base);
    }

    let app = app_element(pid);
    if read_web_text && info.web_tree {
        enable_web_tree_once(&app, pid);
    }
    if read_web_text && info.webkit {
        enable_enhanced_ui_once(&app, pid);
    }
    let Some(window) = app.element("AXFocusedWindow").or_else(|| app.element("AXMainWindow")) else {
        return Some(base);
    };
    let window_title = window.string("AXTitle").unwrap_or_default();

    let title_only = crate::clean::is_title_only(&base.app_name, info.bundle_id.as_deref());

    // Web content lives under AXWebArea; everything outside it is browser
    // chrome (tabs, address bar, bookmarks).
    let web_area = find_web_area(&window);
    let url = web_area
        .as_ref()
        .and_then(|w| w.string("AXURL"))
        .or_else(|| window.string("AXDocument"))
        .filter(|u| !crate::clean::is_internal_url(u));

    if !with_text || title_only {
        return Some(Observation { window_title, url, ..base });
    }

    let mut walk = Walk::new();
    walk.visit(web_area.as_ref().unwrap_or(&window), 0);
    let text = walk.lines.join("\n");
    // A web page whose tree is still building yields nothing: report no
    // text so the caller retries on the next poll instead of storing chrome.
    let text = (!text.is_empty()).then_some(text);

    Some(Observation { window_title, url, text, ..base })
}

/// Shallow search for the page's web area. Browser windows nest it a few
/// groups deep next to the toolbar; give up quickly for native apps.
fn find_web_area(window: &Element) -> Option<Element> {
    fn go(el: &Element, depth: usize, budget: &mut usize) -> Option<Element> {
        if depth > 14 || *budget == 0 {
            return None;
        }
        *budget -= 1;
        let role = el.string("AXRole").unwrap_or_default();
        if role == "AXWebArea" {
            return Some(Element(Cf(unsafe { CFRetain(el.0 .0) })));
        }
        if matches!(role.as_str(), "AXToolbar" | "AXMenuBar" | "AXScrollBar") {
            return None;
        }
        let children = el.children();
        // Chrome's tab strip is an AXTabGroup of AXRadioButtons: skip it.
        // Safari puts the page itself inside an AXTabGroup: look inside.
        if role == "AXTabGroup" && children.first().and_then(|c| c.string("AXRole")).as_deref() == Some("AXRadioButton") {
            return None;
        }
        for child in children {
            if let Some(found) = go(&child, depth + 1, budget) {
                return Some(found);
            }
        }
        None
    }
    let mut budget = 300;
    go(window, 0, &mut budget)
}

fn app_element(pid: i32) -> Element {
    let app = Element(Cf(unsafe { AXUIElementCreateApplication(pid) }));
    unsafe { AXUIElementSetMessagingTimeout(app.0 .0, 0.3) };
    app
}

/// The text of the field being typed in, when the front app exposes it:
/// (app name, text, caret position). Editors and browsers' text areas do;
/// terminals and canvases do not.
pub fn focused_text() -> Option<(String, String)> {
    if !is_trusted(false) {
        return None;
    }
    let (pid, name) = frontmost_window_owner()?;
    let app = app_element(pid);
    let el = app.element("AXFocusedUIElement")?;
    let role = el.string("AXRole").unwrap_or_default();
    if !matches!(role.as_str(), "AXTextArea" | "AXTextField" | "AXComboBox" | "AXWebArea") {
        return None;
    }
    let value = el.string("AXValue")?;
    Some((name.unwrap_or_else(|| app_info(pid).path.unwrap_or_default()), value))
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreateKeyboardEvent(source: *const c_void, virtual_key: u16, key_down: bool) -> *mut c_void;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut c_void);
}

/// ⌘V to the front app (dictation inserts through the clipboard).
pub fn press_paste() {
    const KEY_V: u16 = 9;
    const CMD: u64 = 1 << 20;
    unsafe {
        let down = CGEventCreateKeyboardEvent(std::ptr::null(), KEY_V, true);
        let up = CGEventCreateKeyboardEvent(std::ptr::null(), KEY_V, false);
        if !down.is_null() && !up.is_null() {
            CGEventSetFlags(down, CMD);
            CGEventSetFlags(up, CMD);
            CGEventPost(0, down);
            CGEventPost(0, up);
        }
        if !down.is_null() { CFRelease(down as CFTypeRef) }
        if !up.is_null() { CFRelease(up as CFTypeRef) }
    }
}

/// The system-wide focus query. Fast and exact when it works, but it
/// returns kAXErrorCannotComplete in some process contexts.
fn focused_pid() -> Option<(i32, Option<String>)> {
    if !is_trusted(false) {
        return None;
    }
    let system = Element(Cf(unsafe { AXUIElementCreateSystemWide() }));
    unsafe { AXUIElementSetMessagingTimeout(system.0 .0, 0.3) };
    let pid = system.element("AXFocusedApplication")?.pid()?;
    Some((pid, None))
}

/// Owner of the frontmost normal window, from the window server. Needs no
/// permission.
fn frontmost_window_owner() -> Option<(i32, Option<String>)> {
    unsafe {
        let list = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, 0);
        if list.is_null() {
            return None;
        }
        let list = Cf(list as CFTypeRef);
        let n = CFArrayGetCount(list.0 as CFArrayRef);
        let layer_key = CFString::new("kCGWindowLayer");
        let pid_key = CFString::new("kCGWindowOwnerPID");
        let name_key = CFString::new("kCGWindowOwnerName");
        for i in 0..n {
            let dict = CFArrayGetValueAtIndex(list.0 as CFArrayRef, i) as CFDictionaryRef;
            // Frontmost normal window = first layer-0 window in z-order.
            if dict_i64(dict, &layer_key) != Some(0) {
                continue;
            }
            let Some(pid) = dict_i64(dict, &pid_key).map(|p| p as i32) else { continue };
            let name_ref = CFDictionaryGetValue(dict, name_key.as_CFTypeRef() as *const c_void);
            let name = (!name_ref.is_null()).then(|| CFString::wrap_under_get_rule(name_ref as CFStringRef).to_string());
            return Some((pid, name));
        }
        None
    }
}

/// The same walk, skipping one process: used to look past Lane's own windows.
fn frontmost_other_owner(skip_pid: i32) -> Option<(i32, Option<String>)> {
    unsafe {
        let list = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, 0);
        if list.is_null() {
            return None;
        }
        let list = Cf(list as CFTypeRef);
        let n = CFArrayGetCount(list.0 as CFArrayRef);
        let layer_key = CFString::new("kCGWindowLayer");
        let pid_key = CFString::new("kCGWindowOwnerPID");
        let name_key = CFString::new("kCGWindowOwnerName");
        for i in 0..n {
            let dict = CFArrayGetValueAtIndex(list.0 as CFArrayRef, i) as CFDictionaryRef;
            if dict_i64(dict, &layer_key) != Some(0) {
                continue;
            }
            let Some(pid) = dict_i64(dict, &pid_key).map(|p| p as i32) else { continue };
            if pid == skip_pid {
                continue;
            }
            let name_ref = CFDictionaryGetValue(dict, name_key.as_CFTypeRef() as *const c_void);
            let name = (!name_ref.is_null()).then(|| CFString::wrap_under_get_rule(name_ref as CFStringRef).to_string());
            return Some((pid, name));
        }
        None
    }
}

// ── Accessibility tree walk ──────────────────────────────────────────────

/// Page regions that are navigation, not content (ARIA landmarks as macOS
/// exposes them).
const SKIP_SUBROLES: &[&str] = &[
    "AXSecureTextField",
    "AXLandmarkNavigation",
    "AXLandmarkBanner",
    "AXLandmarkComplementary",
    "AXLandmarkContentInfo",
    "AXLandmarkSearch",
];
const SKIP_ROLES: &[&str] = &["AXSecureTextField", "AXMenuBar", "AXMenu", "AXScrollBar", "AXToolbar", "AXTabGroup"];
const TEXT_ROLES: &[&str] = &["AXStaticText", "AXTextArea", "AXTextField", "AXHeading", "AXLink", "AXCell"];

struct Walk {
    started: Instant,
    nodes: usize,
    chars: usize,
    lines: Vec<String>,
    seen: HashSet<String>,
}

impl Walk {
    fn new() -> Self {
        Self { started: Instant::now(), nodes: 0, chars: 0, lines: Vec::new(), seen: HashSet::new() }
    }

    fn exhausted(&self) -> bool {
        self.nodes >= MAX_NODES || self.chars >= MAX_CHARS || self.started.elapsed() > WALK_BUDGET
    }

    fn push(&mut self, line: String) {
        if self.seen.insert(line.clone()) {
            self.chars += line.len() + 1;
            self.lines.push(line);
        }
    }

    fn visit(&mut self, el: &Element, depth: usize) {
        if depth > MAX_DEPTH || self.exhausted() {
            return;
        }
        self.nodes += 1;

        let role = el.string("AXRole").unwrap_or_default();
        let subrole = el.string("AXSubrole").unwrap_or_default();
        if SKIP_SUBROLES.contains(&subrole.as_str()) || SKIP_ROLES.contains(&role.as_str()) {
            return;
        }

        // A table row becomes one line ("Crosser.pro · 7 · 2025 · available")
        // instead of one line per cell.
        if role == "AXRow" {
            let mut cells = Vec::new();
            self.collect_cells(el, depth + 1, &mut cells);
            if !cells.is_empty() {
                self.push(cells.join(" · "));
            }
            return;
        }

        if TEXT_ROLES.contains(&role.as_str()) {
            if let Some(text) = el.string("AXValue").or_else(|| el.string("AXTitle")) {
                for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    self.push(line.to_string());
                }
            }
        }

        for child in el.children() {
            if self.exhausted() {
                break;
            }
            self.visit(&child, depth + 1);
        }
    }

    fn collect_cells(&mut self, el: &Element, depth: usize, out: &mut Vec<String>) {
        if depth > MAX_DEPTH || self.exhausted() {
            return;
        }
        self.nodes += 1;
        let role = el.string("AXRole").unwrap_or_default();
        if TEXT_ROLES.contains(&role.as_str()) {
            if let Some(text) = el.string("AXValue").or_else(|| el.string("AXTitle")) {
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !text.is_empty() && out.last() != Some(&text) {
                    out.push(text);
                }
            }
        }
        for child in el.children() {
            self.collect_cells(&child, depth + 1, out);
        }
    }
}

// ── Process / bundle helpers ─────────────────────────────────────────────

fn app_info(pid: i32) -> AppInfo {
    let mut guard = APPS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(info) = map.get(&pid) {
        return info.clone();
    }
    let path = process_path(pid).map(|p| bundle_root(&p).to_string());
    let bundle_id = path.as_deref().and_then(bundle_identifier);
    let web_tree = bundle_id.as_deref().is_some_and(|id| CHROMIUM_BUNDLE_PREFIXES.iter().any(|p| id.starts_with(p)));
    let webkit = bundle_id.as_deref().is_some_and(|b| WEBKIT_BUNDLES.contains(&b));
    let info = AppInfo { path, bundle_id, web_tree, webkit };
    if map.len() > 500 {
        map.clear();
    }
    map.insert(pid, info.clone());
    info
}

fn enable_web_tree_once(app: &Element, pid: i32) {
    let mut guard = WEB_TREE_ENABLED.lock().unwrap_or_else(|e| e.into_inner());
    if guard.get_or_insert_with(HashSet::new).insert(pid) {
        // The tree is built asynchronously: text shows up from the next poll.
        app.set_bool("AXManualAccessibility", true);
    }
}

fn enable_enhanced_ui_once(app: &Element, pid: i32) {
    let mut guard = WEB_TREE_ENABLED.lock().unwrap_or_else(|e| e.into_inner());
    if guard.get_or_insert_with(HashSet::new).insert(pid) {
        app.set_bool("AXEnhancedUserInterface", true);
    }
}

unsafe fn dict_i64(dict: CFDictionaryRef, key: &CFString) -> Option<i64> {
    let v = CFDictionaryGetValue(dict, key.as_CFTypeRef() as *const c_void);
    if v.is_null() {
        return None;
    }
    let mut out: i64 = 0;
    CFNumberGetValue(v as CFNumberRef, kCFNumberSInt64Type, &mut out as *mut i64 as *mut c_void).then_some(out)
}

pub fn process_path(pid: i32) -> Option<String> {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let len = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
    (len > 0).then(|| String::from_utf8_lossy(&buf[..len as usize]).to_string())
}

/// Executable path → enclosing bundle. Handles Chrome's update clones
/// (".../Google Chrome.app.bundle/Contents/MacOS/Google Chrome").
pub fn bundle_root(path: &str) -> &str {
    for marker in [".app.bundle/", ".app/"] {
        if let Some(i) = path.find(marker) {
            return &path[..i + marker.len() - 1];
        }
    }
    path
}

pub fn bundle_stem(path: &str) -> Option<String> {
    let last = path.rsplit('/').next()?;
    Some(last.trim_end_matches(".bundle").trim_end_matches(".app").to_string()).filter(|s| !s.is_empty())
}

fn bundle_identifier(bundle_path: &str) -> Option<String> {
    let url = CFURL::from_path(bundle_path, true)?;
    unsafe {
        let bundle = CFBundleCreate(std::ptr::null(), url.as_concrete_TypeRef());
        if bundle.is_null() {
            return None;
        }
        let bundle = Cf(bundle as CFTypeRef);
        let id = CFBundleGetIdentifier(bundle.0 as _);
        (!id.is_null()).then(|| CFString::wrap_under_get_rule(id).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_root_handles_apps_and_update_clones() {
        assert_eq!(
            bundle_root("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            "/Applications/Google Chrome.app"
        );
        assert_eq!(
            bundle_root("/private/var/folders/x/code_sign_clone.3fj/Google Chrome.app.bundle/Contents/MacOS/Google Chrome"),
            "/private/var/folders/x/code_sign_clone.3fj/Google Chrome.app.bundle"
        );
        assert_eq!(bundle_root("/usr/bin/python3"), "/usr/bin/python3");
    }

    #[test]
    fn bundle_stem_strips_suffixes() {
        assert_eq!(bundle_stem("/Applications/Google Chrome.app").as_deref(), Some("Google Chrome"));
        assert_eq!(bundle_stem("/tmp/Google Chrome.app.bundle").as_deref(), Some("Google Chrome"));
        assert_eq!(bundle_stem("/usr/bin/python3").as_deref(), Some("python3"));
    }

    #[test]
    fn reads_bundle_identifier_of_system_app() {
        assert_eq!(bundle_identifier("/System/Applications/Notes.app").as_deref(), Some("com.apple.Notes"));
    }

    #[test]
    fn own_process_path_resolves() {
        assert!(process_path(std::process::id() as i32).is_some());
    }

    #[test]
    fn idle_seconds_is_finite() {
        assert!(idle_seconds().is_finite());
    }
}

#[cfg(test)]
mod live {
    //! Manual checks against real apps (need Accessibility trust):
    //!   cargo test live_observe -- --ignored --nocapture
    //!   RAT_PID=<pid> RAT_MODE=none|manual cargo test live_app -- --ignored --nocapture
    use super::*;

    #[test]
    #[ignore]
    fn live_observe_frontmost() {
        println!("trusted: {} works: {}", is_trusted(false), accessibility_works());
        let t = Instant::now();
        let o = observe(true, true);
        println!("took: {:?}", t.elapsed());
        if let Some(o) = o {
            println!("app: {} {:?} {:?}\ntitle: {:?}\nurl: {:?}", o.app_name, o.bundle_id, o.app_path, o.window_title, o.url);
            let text = o.text.unwrap_or_default();
            println!("text chars: {}\n---\n{}", text.len(), text.chars().take(600).collect::<String>());
        }
    }

    #[test]
    #[ignore]
    fn live_focus_error() {
        let system = Element(Cf(unsafe { AXUIElementCreateSystemWide() }));
        for attr in ["AXFocusedApplication", "AXFocusedUIElement"] {
            let key = CFString::new(attr);
            let mut value: CFTypeRef = std::ptr::null();
            let err = unsafe { AXUIElementCopyAttributeValue(system.0 .0, key.as_concrete_TypeRef(), &mut value) };
            if !value.is_null() { unsafe { CFRelease(value) } }
            println!("{attr}: err={err}");
        }
    }

    /// Dump roles of an app's first window, to see where its text hides.
    #[test]
    #[ignore]
    fn live_tree() {
        let pid: i32 = std::env::var("RAT_PID").unwrap().parse().unwrap();
        let app = Element(Cf(unsafe { AXUIElementCreateApplication(pid) }));
        unsafe { AXUIElementSetMessagingTimeout(app.0 .0, 0.5) };
        if std::env::var("RAT_MODE").as_deref() == Ok("enhanced") {
            app.set_bool("AXEnhancedUserInterface", true);
            std::thread::sleep(Duration::from_millis(1500));
        }
        let win = app.element("AXFocusedWindow").or_else(|| app.element("AXMainWindow")).or_else(|| app.element_at("AXWindows", 0)).expect("window");
        fn dump(el: &Element, depth: usize, count: &mut usize) {
            if depth > 12 || *count > 400 { return; }
            *count += 1;
            let role = el.string("AXRole").unwrap_or_default();
            let sub = el.string("AXSubrole").unwrap_or_default();
            let title = el.string("AXTitle").or_else(|| el.string("AXDescription")).unwrap_or_default();
            let value = el.string("AXValue").map(|v| v.chars().take(40).collect::<String>()).unwrap_or_default();
            let kids = el.children();
            println!("{}{role} {sub} [{}] title={title:?} value={value:?}", "  ".repeat(depth), kids.len());
            for k in kids.iter().take(25) { dump(k, depth + 1, count); }
        }
        let mut n = 0;
        dump(&win, 0, &mut n);
        println!("nodes: {n}");
    }

    #[test]
    #[ignore]
    fn live_app() {
        let pid: i32 = std::env::var("RAT_PID").unwrap().parse().unwrap();
        let app = Element(Cf(unsafe { AXUIElementCreateApplication(pid) }));
        unsafe { AXUIElementSetMessagingTimeout(app.0 .0, 0.5) };
        if std::env::var("RAT_MODE").as_deref() == Ok("manual") {
            app.set_bool("AXManualAccessibility", true);
            std::thread::sleep(Duration::from_millis(1500));
        }
        if std::env::var("RAT_MODE").as_deref() == Ok("enhanced") {
            app.set_bool("AXEnhancedUserInterface", true);
            std::thread::sleep(Duration::from_millis(1500));
        }
        let info = app_info(pid);
        println!("app: {:?} bundle: {:?} web_tree: {}", app.string("AXTitle"), info.bundle_id, info.web_tree);
        let win = app.element("AXFocusedWindow").or_else(|| app.element("AXMainWindow")).or_else(|| app.element_at("AXWindows", 0));
        let Some(win) = win else {
            println!("no window at all");
            return;
        };
        let web = find_web_area(&win);
        let mut walk = Walk::new();
        walk.visit(web.as_ref().unwrap_or(&win), 0);
        let text = walk.lines.join("\n");
        println!(
            "title: {:?} url: {:?} web_area: {} nodes: {} chars: {} took: {:?}\n---\n{}",
            win.string("AXTitle"), web.as_ref().and_then(|w| w.string("AXURL")), web.is_some(), walk.nodes, text.len(), walk.started.elapsed(),
            text.chars().take(1200).collect::<String>()
        );
    }
}
