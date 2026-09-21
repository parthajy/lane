pub mod backup;
pub mod capture;
pub mod clean;
pub mod clipboard;
pub mod diag;
pub mod diarize;
pub mod dictation;
pub mod shots;
pub mod signals;
mod commands;
pub mod connectors;
pub mod engine;
pub mod files;
pub mod icons;
pub mod licence;
pub mod integrations;
pub mod mcp;
pub mod meetings;
pub mod permissions;
pub mod privacy;
pub mod runtime;
pub mod store;
pub mod vault;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent, Wry};

#[derive(Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    pub trusted: bool,
    pub idle: bool,
    pub excluded: bool,
    pub current_app: Option<String>,
    pub current_title: Option<String>,
    pub last_capture_at: Option<i64>,
}

pub struct AppState {
    pub store: Mutex<store::Store>,
    pub engine: Mutex<engine::EngineStatus>,
    pub engine_wake: AtomicBool,
    /// True when the trial has run out and no key has been entered. Capture
    /// and the memory engine stand down; nothing already remembered is lost.
    pub blocked: AtomicBool,
    /// Accessibility was granted to a previous build; needs re-approval.
    pub stale_grant: AtomicBool,
    /// A question is being answered; the engine yields the model.
    pub ask_active: AtomicBool,
    /// Folders changed or settings changed: the next idle pass rescans.
    pub rescan_files: Arc<AtomicBool>,
    /// Files waiting to be read, from the last scan.
    pub file_queue: Mutex<std::collections::VecDeque<files::Candidate>>,
    pub file_watcher: Mutex<Option<notify::RecommendedWatcher>>,
    pub recording: Arc<Mutex<meetings::RecordingStatus>>,
    pub settings: Mutex<store::Settings>,
    pub paused: AtomicBool,
    pub status: Mutex<CaptureStatus>,
    /// The last thing the capture loop saw in front of the user. "Capture my
    /// screen" falls back to this, because clicking the menu bar makes Lane
    /// itself the front app and Lane never captures Lane.
    pub last_seen: Mutex<Option<capture::Observation>>,
    pub db_path: std::path::PathBuf,
    /// Lane.app/Contents/Resources in the installed app.
    pub resource_dir: Option<std::path::PathBuf>,
    pub pause_item: Mutex<Option<MenuItem<Wry>>>,
    pub meeting_item: Mutex<Option<MenuItem<Wry>>>,
    /// What was on screen when the overlay opened (for "ask about this").
    pub screen: Mutex<Option<ScreenContext>>,
}

/// A panic on one thread must not freeze the others: recover the value
/// behind a poisoned mutex instead of giving up.
/// The app's data folder, for processes that run without Tauri (lane-mcp).
pub fn data_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Application Support/so.lane.app")
}

/// Poison-tolerant lock that also tells on itself: a wait over a second is
/// logged with the waiting thread and the type, so a stall is diagnosable
/// from lane.log instead of guessed at.
pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.try_lock() {
        Ok(g) => return g,
        Err(std::sync::TryLockError::Poisoned(e)) => return e.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => {}
    }
    let start = std::time::Instant::now();
    let mut warned = false;
    loop {
        match m.try_lock() {
            Ok(g) => {
                if warned {
                    log::warn!("lock: {} waited {:.1}s for {}", thread_name(), start.elapsed().as_secs_f32(), std::any::type_name::<T>());
                }
                return g;
            }
            Err(std::sync::TryLockError::Poisoned(e)) => return e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                if !warned && start.elapsed() > std::time::Duration::from_secs(1) {
                    warned = true;
                    log::warn!("lock: {} is waiting for {} (held elsewhere for over 1s)", thread_name(), std::any::type_name::<T>());
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

fn thread_name() -> String {
    std::thread::current().name().unwrap_or("unnamed thread").to_string()
}

impl AppState {
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        if let Some(item) = lock(&self.pause_item).clone() {
            let _ = item.set_text(if paused { "Resume capture" } else { "Pause capture" });
        }
    }
}

/// The recall overlay: a floating panel on every Space, hidden from screen
/// sharing and recordings, toggled with the global hotkey.
/// The front window's text at the moment the overlay opened.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenContext {
    pub app_name: String,
    pub window_title: String,
    pub url: Option<String>,
    pub text: String,
    pub captured_at: i64,
}

/// Read the front window before the overlay takes focus. Respects the same
/// exclusions as capture (private apps and sites yield nothing).
fn snapshot_screen(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>();
    let settings = lock(&state.settings).clone();
    let ctx = capture::platform::observe(settings.read_browser_text, true).and_then(|o| {
        if privacy::is_excluded(&settings, &o.app_name, o.bundle_id.as_deref(), o.url.as_deref()) {
            return None;
        }
        let text = o.text.map(|t| clean::clean_text(&t, |_| 0)).unwrap_or_default();
        (!text.trim().is_empty()).then(|| ScreenContext { app_name: o.app_name, window_title: o.window_title, url: o.url, text, captured_at: capture::now_ms() })
    });
    *lock(&state.screen) = ctx;
}

pub fn toggle_overlay(app: &AppHandle, want: Option<bool>) {
    let Some(w) = app.get_webview_window("overlay") else { return };
    let visible = w.is_visible().unwrap_or(false);
    let show = want.unwrap_or(!visible);
    log::info!("overlay: {}", if show { "show" } else { "hide" });
    if show {
        snapshot_screen(app);
        let _ = w.set_content_protected(true);
        raise_window(&w, NS_FLOATING_WINDOW_LEVEL, None, None);
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("overlay-shown", ());
        log::info!("overlay: {}", window_report(&w));
    } else {
        let _ = w.hide();
    }
}

/// Window levels, as AppKit numbers them. Tauri's "always on top" is the
/// floating level, which other apps' windows and full-screen Spaces can
/// still hide; the notch sits at the status level (over the menu bar) and
/// both it and the overlay join every Space, full-screen ones included.
const NS_FLOATING_WINDOW_LEVEL: isize = 3;
const NS_STATUS_WINDOW_LEVEL: isize = 25;

/// Where the notch tab is pinned on its screen.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum NotchPin {
    TopCenter,
    TopLeft,
    TopRight,
    BottomCenter,
    Left,
    Right,
}

impl NotchPin {
    pub fn parse(s: &str) -> NotchPin {
        match s {
            "top-left" => NotchPin::TopLeft,
            "top-right" => NotchPin::TopRight,
            "bottom-center" => NotchPin::BottomCenter,
            "left" => NotchPin::Left,
            "right" => NotchPin::Right,
            _ => NotchPin::TopCenter,
        }
    }
}

/// Other apps that live in the notch (NotchOwl, NotchNook, boring.notch,
/// Notch Buddy, Alcove…), by name; Lane steps aside for them.
#[cfg(target_os = "macos")]
pub fn running_notch_apps() -> Vec<String> {
    use objc2_app_kit::NSWorkspace;
    let ws = unsafe { NSWorkspace::sharedWorkspace() };
    let mut out = Vec::new();
    for a in unsafe { ws.runningApplications() }.iter() {
        let Some(name) = (unsafe { a.localizedName() }) else { continue };
        let name = name.to_string();
        let l = name.to_lowercase();
        if (l.contains("notch") || l == "alcove") && !l.contains("lane") && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}
#[cfg(not(target_os = "macos"))]
pub fn running_notch_apps() -> Vec<String> {
    Vec::new()
}

/// `top_centre`: also pin the window to the top centre of its screen, over
/// the menu bar. Done with `setFrameOrigin` in AppKit coordinates because
/// every Tauri/AppKit "top-left" placement is constrained to sit below the
/// menu bar, whatever the level.
#[cfg(target_os = "macos")]
fn raise_window(w: &tauri::WebviewWindow, level: isize, pin: Option<NotchPin>, size: Option<(f64, f64)>) {
    // Before the event loop runs (setup), `run_on_main_thread` is only
    // queued, and the window would be ordered on screen first with the
    // wrong behaviours, which macOS never revisits. On the main thread the
    // work is done inline.
    // Always queued: Tauri's own window calls (position, show) are queued
    // messages too, so this keeps their order and lands before `show`.
    let w2 = w.clone();
    let _ = w.run_on_main_thread(move || raise_window_now(&w2, level, pin, size));
}

#[cfg(target_os = "macos")]
fn raise_window_now(w2: &tauri::WebviewWindow, level: isize, pin: Option<NotchPin>, size: Option<(f64, f64)>) {
    {
        if let Ok(ptr) = w2.ns_window() {
            use objc2_app_kit::{NSScreen, NSWindow, NSWindowCollectionBehavior};
            let win: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
            // Transparent windows still get an opaque backing on macOS, which
            // showed as white corners around the notch card on a dark desktop.
            win.setOpaque(false);
            unsafe { win.setBackgroundColor(Some(&objc2_app_kit::NSColor::clearColor())) };
            // Become an NSPanel, the class every floating-over-full-screen
            // utility uses. Measured 2026-09-20: a Tauri NSWindow with the
            // same level and behaviours is never drawn over another app's
            // full-screen Space; re-classed as a non-activating panel it is.
            // Same instance layout, so the object is re-classed in place
            // (the technique of the tauri-nspanel plugin).
            if win.class().name().to_str().unwrap_or("") != "LanePanel" {
                unsafe {
                    use objc2::runtime::AnyObject;
                    let obj: *mut AnyObject = ptr as *mut AnyObject;
                    objc2::ffi::object_setClass(obj as *mut objc2::ffi::objc_object, lane_panel_class() as *const _ as *const objc2::ffi::objc_class);
                    let panel: &objc2_app_kit::NSPanel = &*(ptr as *const objc2_app_kit::NSPanel);
                    panel.setStyleMask(panel.styleMask() | objc2_app_kit::NSWindowStyleMask::NonactivatingPanel);
                    panel.setFloatingPanel(true);
                    // Both may take the keyboard when clicked (the card has an ask line);
                    // non-activating, so the app in front keeps its focus otherwise.
                    panel.setBecomesKeyOnlyIfNeeded(false);
                }
                log::info!("{}: re-classed as LanePanel", w2.label());
            }
            win.setLevel(level);
            win.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::FullScreenAuxiliary | NSWindowCollectionBehavior::Stationary | NSWindowCollectionBehavior::IgnoresCycle,
            );
            win.setHidesOnDeactivate(false);
            if let Some(pin) = pin {
                // Size first, in the same AppKit step, so the origin below is
                // computed from the frame the window is about to have. A
                // Tauri set_size goes through another queue and could land
                // after this, leaving the tab pinned by its old size.
                let vertical = matches!(pin, NotchPin::Left | NotchPin::Right);
                let cur = win.frame().size;
                let want = match size {
                    Some((sw, sh)) => Some((sw, sh)),
                    None => {
                        let is_tab = (cur.width <= 20.0 && cur.height >= 100.0) || (cur.height <= 20.0 && cur.width >= 100.0);
                        let tab = if vertical { (14.0, 180.0) } else { (180.0, 14.0) };
                        if is_tab && (cur.width, cur.height) != tab { Some(tab) } else { None }
                    }
                };
                if let Some((sw, sh)) = want {
                    if (cur.width, cur.height) != (sw, sh) {
                        win.setContentSize(objc2_foundation::NSSize::new(sw, sh));
                    }
                }
                let screen = win.screen().or_else(|| NSScreen::mainScreen(unsafe { objc2::MainThreadMarker::new_unchecked() }));
                if let Some(screen) = screen {
                    // Modern MacBooks have a notch of their own. Its height is
                    // the screen's top safe-area inset (0 on every other Mac),
                    // and anything drawn inside it is hidden by the camera
                    // housing, so Lane's tab and card start below it.
                    // LANE_FAKE_NOTCH=<points> pretends this Mac has one, so the
                    // placement can be checked on a machine without a camera housing.
                    let housing = std::env::var("LANE_FAKE_NOTCH")
                        .ok()
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or_else(|| screen.safeAreaInsets().top)
                        .max(0.0);
                    // Full frame for the top centre (over the menu bar, where
                    // the notch is); the visible frame elsewhere, so menus,
                    // status items and the Dock stay clear.
                    let sf = screen.frame();
                    let vf = screen.visibleFrame();
                    let wf = win.frame();
                    let (w, h) = (wf.size.width, wf.size.height);
                    let (x, y) = match pin {
                        NotchPin::TopCenter => (sf.origin.x + (sf.size.width - w) / 2.0, sf.origin.y + sf.size.height - h - housing),
                        NotchPin::TopLeft => (vf.origin.x + 8.0, vf.origin.y + vf.size.height - h),
                        NotchPin::TopRight => (vf.origin.x + vf.size.width - w - 8.0, vf.origin.y + vf.size.height - h),
                        NotchPin::BottomCenter => (sf.origin.x + (sf.size.width - w) / 2.0, vf.origin.y),
                        NotchPin::Left => (vf.origin.x, vf.origin.y + (vf.size.height - h) / 2.0),
                        NotchPin::Right => (vf.origin.x + vf.size.width - w, vf.origin.y + (vf.size.height - h) / 2.0),
                    };
                    win.setFrameOrigin(objc2_foundation::NSPoint::new(x, y));
                    if housing > 0.0 {
                        log::info!("{}: {housing}pt below the camera housing", w2.label());
                    }
                }
            }
            // A behaviour change only takes effect once the window is
            // ordered again: out and back in, so the window server
            // re-evaluates which Spaces it belongs to (measured: an
            // orderFront alone is not enough).
            if win.isVisible() {
                win.orderOut(None);
            }
            win.orderFrontRegardless();
        }
    }
}
#[cfg(not(target_os = "macos"))]
fn raise_window(_w: &tauri::WebviewWindow, _level: isize, _pin: Option<NotchPin>, _size: Option<(f64, f64)>) {}

/// The webview's mouseleave is not reliable once the notch has grown and
/// re-centred under the pointer, so the pointer is watched from here: four
/// times a second, `notch-mouse {inside}` is sent whenever the answer changes.
#[cfg(target_os = "macos")]
fn watch_notch_mouse(app: AppHandle) {
    std::thread::Builder::new()
        .name("notch-mouse".into())
        .spawn(move || {
            let mut was_inside: Option<bool> = None;
            let mut was_full: Option<bool> = None;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let Some(w) = app.get_webview_window("notch") else { continue };

                // The tab sits exactly where you move the pointer to bring the
                // menu bar back in full screen, and being above the menu bar it
                // swallows that hover. So while Lane's own window is full
                // screen the tab steps aside; it comes back on the way out.
                let full = app.get_webview_window("main").and_then(|m| m.is_fullscreen().ok()).unwrap_or(false);
                if was_full != Some(full) {
                    was_full = Some(full);
                    let wants = lock(&app.state::<Arc<AppState>>().settings).notch_enabled;
                    if full {
                        let _ = w.hide();
                        log::info!("notch: hidden while the window is full screen");
                    } else if wants {
                        place_notch(&app);
                        log::info!("notch: back, the window left full screen");
                    }
                }

                if !w.is_visible().unwrap_or(false) {
                    continue;
                }
                let (tx, rx) = std::sync::mpsc::channel();
                let w2 = w.clone();
                let _ = app.run_on_main_thread(move || {
                    let inside = (|| {
                        let ptr = w2.ns_window().ok()?;
                        let win: &objc2_app_kit::NSWindow = unsafe { &*(ptr as *const objc2_app_kit::NSWindow) };
                        let m = objc2_app_kit::NSEvent::mouseLocation();
                        let f = win.frame();
                        // A little slack below the card so a slow exit does not flicker.
                        Some(m.x >= f.origin.x - 4.0 && m.x <= f.origin.x + f.size.width + 4.0 && m.y >= f.origin.y - 6.0 && m.y <= f.origin.y + f.size.height + 2.0)
                    })();
                    let _ = tx.send(inside);
                });
                let Ok(Some(inside)) = rx.recv_timeout(std::time::Duration::from_millis(500)) else { continue };
                if was_inside != Some(inside) {
                    was_inside = Some(inside);
                    log::info!("notch-mouse: inside={inside}");
                    let _ = w.emit("notch-mouse", serde_json::json!({"inside": inside}));
                }
            }
        })
        .expect("spawn notch-mouse");
}
#[cfg(not(target_os = "macos"))]
fn watch_notch_mouse(_app: AppHandle) {}

/// `LanePanel`: an NSPanel that can take the keyboard although it has no
/// title bar (a plain borderless NSPanel cannot, and the overlay's search
/// field needs it). Registered once, at first use.
#[cfg(target_os = "macos")]
fn lane_panel_class() -> &'static objc2::runtime::AnyClass {
    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    static CLASS: std::sync::OnceLock<&'static AnyClass> = std::sync::OnceLock::new();
    CLASS.get_or_init(|| {
        extern "C" fn yes(_this: &AnyObject, _sel: Sel) -> Bool {
            Bool::YES
        }
        extern "C" fn no(_this: &AnyObject, _sel: Sel) -> Bool {
            Bool::NO
        }
        let superclass = <objc2_app_kit::NSPanel as objc2::ClassType>::class();
        let mut builder = ClassBuilder::new(c"LanePanel", superclass).expect("LanePanel class name free");
        unsafe {
            builder.add_method(objc2::sel!(canBecomeKeyWindow), yes as extern "C" fn(_, _) -> _);
            builder.add_method(objc2::sel!(canBecomeMainWindow), no as extern "C" fn(_, _) -> _);
        }
        builder.register()
    })
}

/// Every NSWindow of the process, one line each (self-tests, diagnostics).
#[cfg(target_os = "macos")]
pub fn windows_dump(app: &AppHandle) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = app.run_on_main_thread(move || {
        use objc2_app_kit::{NSApplication, NSWindowOcclusionState};
        let mtm = unsafe { objc2::MainThreadMarker::new_unchecked() };
        let app = NSApplication::sharedApplication(mtm);
        let mut out = format!("policy={:?}\n", app.activationPolicy());
        for win in app.windows().iter() {
            let occluded = !win.occlusionState().contains(NSWindowOcclusionState::Visible);
            let f = win.frame();
            out.push_str(&format!(
                "  #{} {} title={:?} visible={} level={} behaviour={:#x} activeSpace={} occluded={} sharing={:?} style={:#x} frame=({},{} {}x{})\n",
                win.windowNumber(), win.class().name().to_str().unwrap_or("?"), win.title().to_string(), win.isVisible(), win.level(), win.collectionBehavior().0, win.isOnActiveSpace(), occluded, win.sharingType().0, win.styleMask().0, f.origin.x, f.origin.y, f.size.width, f.size.height
            ));
        }
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or_default()
}
#[cfg(not(target_os = "macos"))]
pub fn windows_dump(_app: &AppHandle) -> String {
    String::new()
}

/// One line on where a window is and whether the user can see it: level,
/// visible, occluded, on the active Space, position and size. Logged when
/// the notch is placed and the overlay shown, and by the diagnostics bundle.
pub fn window_report(w: &tauri::WebviewWindow) -> String {
    let pos = w.outer_position().map(|p| format!("{},{}", p.x, p.y)).unwrap_or_else(|_| "?".into());
    let size = w.outer_size().map(|s| format!("{}x{}", s.width, s.height)).unwrap_or_else(|_| "?".into());
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let w2 = w.clone();
        let _ = w.run_on_main_thread(move || {
            let mut line = String::new();
            if let Ok(ptr) = w2.ns_window() {
                use objc2_app_kit::{NSWindow, NSWindowOcclusionState};
                let win: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
                let occluded = !win.occlusionState().contains(NSWindowOcclusionState::Visible);
                line = format!("level={} visible={} occluded={} activeSpace={} behaviour={:#x}", win.level(), win.isVisible(), occluded, win.isOnActiveSpace(), win.collectionBehavior().0);
            }
            let _ = tx.send(line);
        });
        let extra = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or_default();
        format!("{} at {pos} {size} {extra}", w.label())
    }
    #[cfg(not(target_os = "macos"))]
    format!("{} at {pos} {size}", w.label())
}

/// Put the notch tab at the top centre of the screen it is on, hidden from
/// screen sharing, on every Space, above the menu bar. `width` is the size
/// it is about to have (the window may not report a new size yet).
pub fn place_notch(app: &AppHandle) {
    place_notch_size(app, None);
}

/// `size`: the logical size the notch is about to have (tab or card), applied
/// in the same AppKit step as the placement.
pub fn place_notch_size(app: &AppHandle, size: Option<(f64, f64)>) {
    let Some(w) = app.get_webview_window("notch") else { return };
    // LANE_UNPROTECTED=1 (self-tests only) leaves the notch visible to
    // screenshots so its placement can be checked from outside.
    let _ = w.set_content_protected(std::env::var("LANE_UNPROTECTED").is_err());
    // Level and Space behaviour are set in `raise_window`; Tauri's
    // always-on-top / all-workspaces calls would queue behind it and undo it.
    let pin = app.try_state::<Arc<AppState>>().map(|s| NotchPin::parse(&lock(&s.settings).notch_position)).unwrap_or(NotchPin::TopCenter);
    // Placement is done in AppKit (`raise_window`): a Tauri `set_position`
    // here goes through a different main-thread queue (dispatch_async) than
    // the AppKit step (event-loop proxy), so it could land afterwards and
    // drag the tab back to the top centre whatever the chosen spot.
    #[cfg(not(target_os = "macos"))]
    {
        let monitor = w.current_monitor().ok().flatten().or_else(|| w.primary_monitor().ok().flatten());
        if let Some(m) = monitor {
            let scale = m.scale_factor();
            let screen_w = m.size().width as f64 / scale;
            let cur = size.map(|s| s.0).unwrap_or(180.0);
            let x = m.position().x as f64 / scale + ((screen_w - cur) / 2.0).max(0.0);
            let _ = w.set_position(tauri::LogicalPosition::new(x, m.position().y as f64 / scale));
        }
    }
    raise_window(&w, NS_STATUS_WINDOW_LEVEL, Some(pin), size);
    log::info!("notch: {}", window_report(&w));
}

pub fn notch_visible(app: &AppHandle, on: bool) {
    let Some(w) = app.get_webview_window("notch") else { return };
    if on {
        place_notch(app);
        match w.show() {
            Ok(()) => log::info!("notch: shown at {:?}", w.outer_position().ok()),
            Err(e) => log::warn!("notch: could not show: {e}"),
        }
    } else {
        let _ = w.hide();
    }
}

/// Lane is a menu-bar app (Accessory policy) from the first frame and
/// stays one: a window shown by a Regular app is never allowed onto other
/// apps' full-screen Spaces, and switching policy later does not recover it
/// (measured 2026-09-20). The main window opens from the tray, the notch or
/// a hotkey and takes focus like any window; there is no Dock icon.
/// Write down whatever is in front, on purpose, and say so in the notch.
pub fn capture_now_from(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>().inner().clone();
    let app2 = app.clone();
    std::thread::spawn(move || {
        match commands::capture_now_inner(&app2, &state) {
            Ok(title) => engine::notch(&app2, "brief", "Captured", vec![title]),
            Err(e) => {
                log::warn!("capture: {e}");
                engine::notch(&app2, "help", "Nothing captured", vec![e]);
            }
        }
    });
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        // Lane runs as an accessory (menu bar) app. From another app's
        // full-screen Space, "Open Lane" must still activate Lane so macOS
        // switches to the Space holding the main window; Tauri's focus
        // alone leaves the window ordered but the user staring at the
        // full-screen app (measured 2026-09-20).
        #[cfg(target_os = "macos")]
        {
            let w2 = w.clone();
            let _ = app.run_on_main_thread(move || {
                if let Ok(ptr) = w2.ns_window() {
                    use objc2_app_kit::{NSApplication, NSWindow};
                    use objc2_foundation::MainThreadMarker;
                    let win: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
                    if let Some(mtm) = MainThreadMarker::new() {
                        #[allow(deprecated)]
                        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
                    }
                    win.makeKeyAndOrderFront(None);
                    log::info!("main: shown (level={} visible={})", win.level(), win.isVisible());
                }
            });
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch (dev build + installed app, or double-click)
        // focuses the running copy instead of capturing in parallel.
        // A second launch brings the window up; `open -a Lane --args --page settings`
        // also lands on a page (used by the self-tests and by links).
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            show_main(app);
            if let Some(i) = args.iter().position(|a| a == "--page") {
                if let Some(page) = args.get(i + 1) {
                    let _ = app.emit("navigate", serde_json::json!({"page": page, "activity": null}));
                }
            }
            // `rat-mac --ask "question"`: the self-test path for Ask. The
            // running app answers and writes <data>/asks/<n>.txt; nothing is
            // shown or kept in the conversation history.
            if let Some(i) = args.iter().position(|a| a == "--ask") {
                if let Some(q) = args.get(i + 1).cloned() {
                    let state = app.state::<Arc<AppState>>().inner().clone();
                    std::thread::spawn(move || {
                        let dir = state.db_path.with_file_name("asks");
                        let _ = std::fs::create_dir_all(&dir);
                        let stamp = capture::now_ms();
                        let path = dir.join(format!("{stamp}.txt"));
                        // Like the UI path: the memory engine yields the model
                        // while a question is answered.
                        state.ask_active.store(true, std::sync::atomic::Ordering::Relaxed);
                        let result = engine::answer_with(&state, &q, &[], false, |_| {});
                        state.ask_active.store(false, std::sync::atomic::Ordering::Relaxed);
                        let fixed = *lock(&engine::LAST_SCOPE_FIX);
                        let text = match result {
                            Ok((a, sources)) => format!("Q: {q}\nSCOPE_FIXED: {fixed}\nA: {a}\nSOURCES: {}\n", sources.len()),
                            Err(e) => format!("Q: {q}\nERROR: {e}\n"),
                        };
                        let _ = std::fs::write(&path, text);
                        log::info!("ask-eval: {} → {}", q, path.display());
                    });
                }
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Tauri applies the policy when the loop runs, after this setup;
            // the notch is shown below, so AppKit is told right now as well.
            #[cfg(target_os = "macos")]
            {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
                let mtm = unsafe { objc2::MainThreadMarker::new_unchecked() };
                NSApplication::sharedApplication(mtm).setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            }
            // An accessory app does not get the full-screen behaviour for
            // free, so the green button only zoomed. Ask for it explicitly on
            // the main window (the notch and the overlay stay auxiliary).
            #[cfg(target_os = "macos")]
            if let Some(w) = app.get_webview_window("main") {
                if let Ok(ptr) = w.ns_window() {
                    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
                    let win: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
                    win.setCollectionBehavior(win.collectionBehavior() | NSWindowCollectionBehavior::FullScreenPrimary);
                }
            }
            let data_dir = app.path().app_data_dir()?;
            // First launch as Lane: adopt the Reattend-era data folder.
            if !data_dir.join("memory.db").exists() {
                if let Some(old) = data_dir.parent().map(|p| p.join("com.reattend.mac")) {
                    if old.join("memory.db").exists() {
                        if data_dir.exists() {
                            let _ = std::fs::remove_dir(&data_dir);
                        }
                        match std::fs::rename(&old, &data_dir) {
                            Ok(()) => eprintln!("migrated data folder from {}", old.display()),
                            Err(e) => eprintln!("could not migrate data folder: {e}"),
                        }
                    }
                }
            }
            std::fs::create_dir_all(&data_dir)?;
            diag::init(&data_dir.join("lane.log"));
            connectors::ensure_examples(&connectors::dir(&data_dir));
            files::set_ocr_helper(app.path().resource_dir().ok().as_deref());
            shots::init(app.path().resource_dir().ok().as_deref());
            log::info!("Lane {} starting", env!("CARGO_PKG_VERSION"));
            if let Some(dir) = runtime::runtime_dir(app.path().resource_dir().ok().as_deref()) {
                runtime::kill_strays(&dir);
            }
            let db_path = data_dir.join("memory.db");
            let store = store::Store::open(&db_path)?;
            // Earlier builds could record Lane itself and system overlays.
            store.purge_apps(privacy::SYSTEM_EXCLUDED_APPS)?;
            store.purge_apps(&["Reattend", "rat-mac"])?;
            if store.needs_reclean()? {
                let s = store.reclean_all()?;
                log::info!("recleaned {} snapshots: {} → {} chars, {} removed", s.snapshots, s.raw_chars, s.clean_chars, s.removed_empty);
            }
            let settings = store.settings();
            let stale = permissions::reconcile_grant(&settings.trusted_build, !settings.trusted_build.is_empty());

            let state = Arc::new(AppState {
                store: Mutex::new(store),
                engine: Mutex::new(engine::EngineStatus::default()),
                engine_wake: AtomicBool::new(false),
                blocked: AtomicBool::new(false),
                last_seen: Mutex::new(None),
                stale_grant: AtomicBool::new(stale),
                ask_active: AtomicBool::new(false),
                rescan_files: Arc::new(AtomicBool::new(true)),
                file_queue: Mutex::new(std::collections::VecDeque::new()),
                file_watcher: Mutex::new(None),
                recording: Arc::new(Mutex::new(meetings::RecordingStatus::default())),
                settings: Mutex::new(settings),
                paused: AtomicBool::new(false),
                status: Mutex::new(CaptureStatus::default()),
                db_path,
                resource_dir: app.path().resource_dir().ok(),
                pause_item: Mutex::new(None),
                meeting_item: Mutex::new(None),
                screen: Mutex::new(None),
            });
            engine::set_who(&lock(&state.settings).display_name);
            app.manage(state.clone());

            let recall_item = MenuItem::with_id(app, "recall", "Recall…\t⌥ Space", true, None::<&str>)?;
            let meeting_item = MenuItem::with_id(app, "meeting", "Record meeting", true, None::<&str>)?;
            let dictate_item = MenuItem::with_id(app, "dictate", "Dictate\t⌥⇧ Space", true, None::<&str>)?;
            let open_item = MenuItem::with_id(app, "open", "Open Lane", true, None::<&str>)?;
            let pause_item = MenuItem::with_id(app, "pause", "Pause capture", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit Lane", true, None::<&str>)?;
            let capture_item = MenuItem::with_id(app, "capture", "Capture my screen", true, None::<&str>)?;
            let notch_on = lock(&state.settings).notch_enabled;
            let notch_item = CheckMenuItem::with_id(app, "notch", "Show the notch", true, notch_on, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&recall_item, &capture_item, &meeting_item, &dictate_item, &open_item, &pause_item, &notch_item, &separator, &quit_item])?;
            *lock(&state.meeting_item) = Some(meeting_item);

            // Global hotkey for recall. ⌥ Space first; ⌃ ⌥ Space if something else owns it.
            {
                use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
                let handle = app.handle().clone();
                let gs = app.global_shortcut();
                let registered = ["Alt+Space", "Ctrl+Alt+Space"].iter().find(|combo| {
                    let h = handle.clone();
                    gs.on_shortcut(**combo, move |_, _, ev| {
                        if ev.state() == ShortcutState::Pressed {
                            toggle_overlay(&h, None);
                        }
                    })
                    .is_ok()
                });
                match registered {
                    Some(c) => log::info!("recall hotkey: {c}"),
                    None => log::warn!("recall hotkey: could not register"),
                }
                // Dictation: ⌥⇧Space starts, again inserts.
                let h2 = app.handle().clone();
                let s2 = state.clone();
                if gs.on_shortcut("Alt+Shift+Space", move |_, _, ev| {
                    if ev.state() == ShortcutState::Pressed {
                        if let Err(e) = dictation::toggle(&h2, &s2) {
                            log::warn!("dictation: {e}");
                            engine::notch(&h2, "help", "Dictation", vec![e]);
                        }
                    }
                }).is_ok() {
                    log::info!("dictation hotkey: Alt+Shift+Space");
                }
            }
            if let Some(w) = app.get_webview_window("overlay") {
                let _ = w.set_content_protected(true);
                let h = app.handle().clone();
                w.on_window_event(move |e| {
                    if let WindowEvent::Focused(false) = e {
                        toggle_overlay(&h, Some(false));
                    }
                });
            }
            *lock(&state.pause_item) = Some(pause_item);

            let tray_state = state.clone();
            TrayIconBuilder::with_id("main")
                .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("Lane")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "recall" => toggle_overlay(app, Some(true)),
                    "dictate" => {
                        if let Err(e) = dictation::toggle(app, &tray_state) {
                            engine::notch(app, "help", "Dictation", vec![e]);
                        }
                    }
                    "meeting" => {
                        if dictation::active() {
                            engine::notch(app, "help", "Dictation", vec!["A dictation is running. Press ⌥⇧Space to finish it first.".into()]);
                        } else if lock(&meetings::RECORDER).is_some() {
                            engine::stop_meeting(app.clone(), tray_state.clone());
                            if let Some(i) = lock(&tray_state.meeting_item).clone() { let _ = i.set_text("Record meeting"); }
                        } else {
                            match engine::start_meeting(app, &tray_state, String::new(), false) {
                                Ok(_) => { if let Some(i) = lock(&tray_state.meeting_item).clone() { let _ = i.set_text("Stop recording ●"); } }
                                Err(e) => log::warn!("meeting: {e}"),
                            }
                            show_main(app);
                            let _ = app.emit("navigate", serde_json::json!({"page": "meetings"}));
                        }
                    }
                    "open" => show_main(app),
                    "capture" => { capture_now_from(app); }
                    "notch" => {
                        // The checkbox toggles itself; follow it with the setting
                        // and the window, so the two never disagree.
                        let on = !lock(&tray_state.settings).notch_enabled;
                        {
                            let mut st = lock(&tray_state.settings);
                            st.notch_enabled = on;
                            let _ = lock(&tray_state.store).save_settings(&st);
                        }
                        if on { place_notch(app); } else if let Some(w) = app.get_webview_window("notch") { let _ = w.hide(); }
                        log::info!("notch: {} from the menu bar", if on { "on" } else { "off" });
                    }
                    "pause" => {
                        let paused = !tray_state.paused.load(Ordering::Relaxed);
                        tray_state.set_paused(paused);
                    }
                    "quit" => {
                        log::info!("quit from menu bar");
                        let _ = meetings::stop();
                        runtime::shutdown();
                        app.exit(0)
                    }
                    _ => {}
                })
                .build(app)?;

            capture::spawn(app.handle().clone(), state.clone());
            clipboard::spawn(state.clone());
            // A small tab below the notch; it grows into a card on hover. The
            // window is created visible, so switching the notch off has to be
            // applied here too, or it would come back on every launch.
            notch_visible(app.handle(), lock(&state.settings).notch_enabled);
            watch_notch_mouse(app.handle().clone());
            if lock(&state.settings).mail_enabled {
                engine::apply_mail_setting(&state);
            }
            engine::spawn(app.handle().clone(), state.clone());
            engine::start_file_watcher(&state);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps capture running in the menu bar.
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                #[cfg(target_os = "macos")]
                let _ = window.app_handle().set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::source_icon,
            commands::set_fullscreen,
            commands::record_screen,
            commands::capture_now,
            commands::save_png,
            commands::licence_status,
            commands::apply_licence,
            commands::clear_licence,
            commands::get_status,
            commands::set_paused,
            commands::get_permissions,
            commands::request_accessibility,
            commands::open_settings_pane,
            commands::reset_accessibility,
            commands::set_launch_at_login,
            commands::restart_app,
            commands::get_privacy_lists,
            commands::list_activities,
            commands::get_activity,
            commands::search,
            commands::delete_activity,
            commands::get_settings,
            commands::update_settings,
            commands::wipe_all,
            commands::list_memories,
            commands::memory_feedback,
            commands::engine_status,
            commands::process_now,
            commands::export_labels,
            commands::ask,
            commands::recap,
            commands::day_stats,
            commands::list_entities,
            commands::entity_memories,
            commands::graph,
            commands::board,
            commands::set_board_position,
            commands::list_tasks,
            commands::set_task_status,
            commands::start_meeting,
            commands::add_note,
            commands::list_decisions,
            commands::stop_meeting,
            commands::recording_status,
            commands::list_meetings,
            commands::rename_meeting,
            commands::meeting_summary,
            commands::labels_status,
            commands::exclude_label,
            commands::send_labels_now,
            commands::backup_status,
            commands::set_backup_passphrase,
            commands::backup_now,
            commands::choose_backup_file,
            commands::restore_backup,
            commands::disable_backups,
            commands::copy_text,
            commands::save_markdown,
            commands::search_files,
            commands::file_stats,
            commands::reveal_file,
            commands::open_file,
            commands::reindex_files,
            commands::show_overlay,
            commands::hide_overlay,
            commands::open_main,
            commands::upcoming_events,
            commands::notion_status,
            commands::set_notion_token,
            commands::disconnect_notion,
            commands::sync_notion_now,
            commands::export_to_notion,
            commands::entity_profile,
            commands::set_pinned,
            commands::update_memory,
            commands::memory_facts,
            commands::add_fact,
            commands::retract_fact,
            commands::correct_fact,
            commands::conflicting_facts,
            commands::list_connectors,
            commands::run_connector,
            commands::open_connectors_folder,
            commands::set_connector_secret,
            commands::mcp_config,
            commands::upcoming_dates,
            commands::memory_gaps,
            commands::explore,
            commands::set_alias,
            commands::remove_alias,
            commands::aliases_of,
            commands::forget_term,
            commands::screen_context,
            commands::ask_screen,
            commands::diagnostics_bundle,
            commands::quit_app,
            commands::add_board_link,
            commands::set_board_link_label,
            commands::remove_board_link,
            commands::set_board_note,
            commands::remove_board_node,
            commands::memory_detail,
            commands::list_conversations,
            commands::conversation_messages,
            commands::rename_conversation,
            commands::delete_conversation,
            commands::insights,
            commands::dictation_toggle,
            commands::notch_show,
            commands::notch_hide,
            commands::notch_resize,
            commands::meeting_notes,
            commands::open_meeting_audio,
            commands::delete_meeting,
            commands::list_files,
            commands::rename_speaker,
            commands::snapshot_image,
            commands::request_screen_recording,
            commands::speaker_toolkit,
            commands::notch_context,
            commands::signals,
            commands::set_signal_state,
            commands::rank_signals,
            commands::refresh_signals,
            commands::circle,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Lane")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { code, .. } = &event {
                log::info!("exit requested (code {code:?})");
            }
            if let tauri::RunEvent::Exit = event {
                log::info!("exiting");
                runtime::shutdown();
            }
        });
}

#[cfg(test)]
mod hygiene_tests {
    //! Guards against the two bugs that froze the app on 2026-09-19: a lock
    //! taken twice inside one statement (Rust keeps the first guard alive to
    //! the end of the statement, so the second take waits forever), and a
    //! slow command running on the main thread.

    fn sources() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push((p.display().to_string(), std::fs::read_to_string(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")), &mut out);
        out
    }

    #[test]
    fn no_mutex_is_locked_twice_in_one_statement() {
        let mut offenders = Vec::new();
        for (file, text) in sources() {
            // Statements end at ';' or at a block boundary. Test code is checked too.
            for (i, stmt) in text.split(|c| c == ';' || c == '{' || c == '}').enumerate() {
                let mut seen: Vec<&str> = Vec::new();
                let mut rest = stmt;
                while let Some(pos) = rest.find("lock(&") {
                    let after = &rest[pos + 6..];
                    let end = after.find(')').unwrap_or(after.len());
                    let target = after[..end].trim();
                    if seen.contains(&target) {
                        offenders.push(format!("{file}: statement #{i} locks `{target}` twice"));
                    }
                    seen.push(target);
                    rest = &after[end..];
                }
            }
        }
        assert!(offenders.is_empty(), "double locks:\n{}", offenders.join("\n"));
    }

    #[test]
    fn commands_run_off_the_main_thread() {
        // Only window and dialog work belongs on the main thread.
        let allowed = ["show_overlay", "hide_overlay", "open_main", "restart_app", "choose_backup_file", "copy_text", "open_settings_pane", "quit_app", "notch_show", "notch_hide", "notch_resize"];
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/commands.rs")).unwrap();
        let mut offenders = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim() == "#[tauri::command]" {
                let next = text.lines().nth(i + 1).unwrap_or("");
                let name = next.trim_start_matches("pub fn ").split('(').next().unwrap_or("?");
                if !allowed.contains(&name) {
                    offenders.push(name.to_string());
                }
            }
        }
        assert!(offenders.is_empty(), "sync commands that could block the window: {offenders:?}");
    }
}
