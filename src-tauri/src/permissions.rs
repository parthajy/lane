//! Everything the setup screen needs to explain permissions precisely:
//! what macOS will show in System Settings, whether access works, and
//! launch at login.
//!
//! macOS never lets an app switch its own Accessibility permission on. The
//! best we can do is add the app to the list, open the exact settings page
//! and detect the moment the user flips the switch.

use crate::capture::platform;
use serde::Serialize;
use std::path::PathBuf;

pub const BUNDLE_ID: &str = "so.lane.app";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionReport {
    pub accessibility_trusted: bool,
    /// Trusted and calls succeed. Trusted-but-not-working means restart.
    pub accessibility_works: bool,
    /// Installed .app (true) or a development build run from a terminal.
    pub bundled: bool,
    /// The name macOS shows in Privacy & Security → Accessibility.
    pub listed_as: String,
    pub executable: String,
    pub launch_at_login: bool,
    /// Accessibility was granted to an earlier build of this app; macOS
    /// ties the grant to the exact binary, so it must be approved again.
    pub stale_grant: bool,
    /// Screen Recording, needed only for pictures with memories.
    pub screen_recording: bool,
}

/// Identifies this exact build. Ad-hoc signatures change on every build and
/// macOS keys Accessibility grants on them, so a changed id with a lost
/// grant means "approve the new build", not "the user never granted".
pub fn build_id() -> String {
    std::env::current_exe()
        .and_then(|p| std::fs::metadata(p))
        .map(|m| {
            let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            format!("{}-{}", m.len(), t)
        })
        .unwrap_or_default()
}

/// Called at startup with what the settings remember. If a previous build
/// was trusted and this one is not, drop the stale entry so the next
/// request adds a fresh one, and say so.
pub fn reconcile_grant(remembered_build: &str, remembered_trusted: bool) -> bool {
    let trusted = platform::is_trusted(false);
    let stale = !trusted && remembered_trusted && !remembered_build.is_empty() && remembered_build != build_id();
    if stale {
        log::info!("accessibility: grant belongs to build {remembered_build}; resetting for {}", build_id());
        let _ = reset_accessibility();
    }
    stale
}

pub fn report(stale_grant: bool) -> PermissionReport {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    let bundled = exe.contains(".app/Contents/MacOS/");
    let trusted = platform::is_trusted(false);
    PermissionReport {
        screen_recording: crate::shots::granted(),
        stale_grant: stale_grant && !trusted,
        accessibility_trusted: trusted,
        accessibility_works: trusted && platform::accessibility_works(),
        bundled,
        listed_as: if bundled { "Lane".into() } else { responsible_app_name().unwrap_or_else(|| "your terminal".into()) },
        executable: exe,
        launch_at_login: launch_agent_path().is_some_and(|p| p.exists()),
    }
}

/// A development build inherits permissions from the app that launched it
/// (Terminal, VS Code, iTerm…). Walk up the process tree to find it.
#[cfg(target_os = "macos")]
fn responsible_app_name() -> Option<String> {
    let mut pid = std::process::id() as i32;
    for _ in 0..16 {
        pid = parent_pid(pid)?;
        if pid <= 1 {
            return None;
        }
        let path = platform::process_path(pid)?;
        if path.contains(".app/") {
            return platform::bundle_stem(platform::bundle_root(&path));
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn responsible_app_name() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn parent_pid(pid: i32) -> Option<i32> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let n = unsafe {
        libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, &mut info as *mut _ as *mut libc::c_void, size)
    };
    (n == size).then_some(info.pbi_ppid as i32)
}

pub fn open_pane(pane: &str) -> Result<(), String> {
    let url = match pane {
        "accessibility" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "loginItems" => "x-apple.systempreferences:com.apple.LoginItems-Settings.extension",
        _ => return Err(format!("unknown settings pane: {pane}")),
    };
    std::process::Command::new("open").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Adds Lane to the Accessibility list (switch off) with the system
/// prompt, then opens the page where the user turns it on.
pub fn request_accessibility() -> Result<bool, String> {
    let trusted = platform::is_trusted(true);
    if !trusted {
        open_pane("accessibility")?;
    }
    Ok(trusted)
}

/// Removes a stale entry (common after updating an unsigned build), so the
/// next request adds a fresh one. Only meaningful for the installed app.
pub fn reset_accessibility() -> Result<(), String> {
    let status = std::process::Command::new("tccutil")
        .args(["reset", "Accessibility", BUNDLE_ID])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err("tccutil could not reset the permission".into()) }
}

fn launch_agent_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/LaunchAgents").join(format!("{BUNDLE_ID}.plist")))
}

pub fn set_launch_at_login(enabled: bool) -> Result<bool, String> {
    let path = launch_agent_path().ok_or("HOME is not set")?;
    if !enabled {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        return Ok(false);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.display().to_string();
    if !exe.contains(".app/Contents/MacOS/") {
        return Err("Launch at login is available in the installed app, not in development builds.".into());
    }
    std::fs::create_dir_all(path.parent().expect("has parent")).map_err(|e| e.to_string())?;
    std::fs::write(&path, launch_agent_plist(&exe)).map_err(|e| e.to_string())?;
    Ok(true)
}

fn launch_agent_plist(exe: &str) -> String {
    let exe = exe.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{BUNDLE_ID}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
</dict>
</plist>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_escapes_path() {
        let p = launch_agent_plist("/Applications/R&D <x>.app/Contents/MacOS/Lane");
        assert!(p.contains("R&amp;D &lt;x&gt;.app"));
        assert!(p.contains("<key>RunAtLoad</key>"));
    }

    #[test]
    fn dev_build_finds_the_app_that_launched_it() {
        // Under cargo test we are a dev binary; the report must not claim "Lane".
        let r = report(false);
        assert!(!r.bundled);
        assert_ne!(r.listed_as, "Lane");
    }

    #[test]
    fn build_id_is_stable_within_a_process() {
        assert!(!build_id().is_empty());
        assert_eq!(build_id(), build_id());
    }

    #[test]
    fn unknown_pane_is_rejected() {
        assert!(open_pane("camera").is_err());
    }
}

#[cfg(test)]
mod live {
    #[test]
    #[ignore]
    fn live_permission_report() {
        let r = super::report(false);
        println!("trusted={} works={} bundled={} listed_as={:?} login={}", r.accessibility_trusted, r.accessibility_works, r.bundled, r.listed_as, r.launch_at_login);
    }
}
