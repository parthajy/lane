//! Making sure Lane is where it needs to be.
//!
//! Opened straight from the disk image, or from Downloads, macOS either runs
//! Lane off a read-only volume or hides it inside a randomised, throwaway
//! mount of its own (App Translocation). Either way the permissions someone
//! grants attach to a path that will not exist tomorrow, updates cannot
//! replace the app, and nothing that goes wrong afterwards looks related to
//! where it was opened from. It fails quietly, which is the worst way.
//!
//! So: if Lane is not in Applications, it says so and offers to move itself.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the running bundle is, if this is a bundle at all.
pub fn bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // …/Lane.app/Contents/MacOS/rat-mac → …/Lane.app
    let app = exe.parent()?.parent()?.parent()?;
    app.extension().filter(|e| *e == "app").map(|_| app.to_path_buf())
}

/// Why Lane should not stay where it is, in words worth showing someone.
pub fn wrong_place(path: &Path) -> Option<&'static str> {
    let s = path.to_string_lossy();
    if s.contains("/AppTranslocation/") {
        return Some("macOS is running Lane from a temporary copy, which disappears when you quit. Permissions you grant will not stick.");
    }
    if s.starts_with("/Volumes/") {
        return Some("Lane is running from the disk image it arrived in. That disc is read-only and goes away when you eject it.");
    }
    // A development build, run from the build folder, is meant to be there.
    if s.contains("/target/") || std::env::var("LANE_DEV").is_ok() {
        return None;
    }
    if s.starts_with("/Applications/") {
        return None;
    }
    if let Ok(home) = std::env::var("HOME") {
        if s.starts_with(&format!("{home}/Applications/")) {
            return None;
        }
    }
    Some("Lane works best from your Applications folder, where updates and permissions can find it.")
}

/// Copy the bundle to /Applications, start that one, and let this one go.
pub fn move_to_applications(from: &Path) -> Result<PathBuf, String> {
    let to = PathBuf::from("/Applications/Lane.app");
    if to.exists() {
        // An older copy sitting there would otherwise make ditto merge the
        // two, which leaves a bundle that is neither one thing nor the other.
        std::fs::remove_dir_all(&to).map_err(|e| format!("There is already a Lane in Applications and it could not be replaced: {e}"))?;
    }
    // ditto keeps the signature intact; --noqtn so the copy does not get
    // translocated in turn, which would put us right back here.
    let out = Command::new("/usr/bin/ditto")
        .args(["--noqtn", &from.display().to_string(), &to.display().to_string()])
        .output()
        .map_err(|e| format!("Lane could not be copied: {e}"))?;
    if !out.status.success() {
        return Err(format!("Lane could not be copied: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(to)
}

/// Wait for this copy to be gone, then eject the disc and start the new one.
///
/// Order matters, and getting it wrong is not obvious. Ejecting first pulls
/// the running program out from under itself and it will not die cleanly.
/// Starting the new copy first means two Lanes exist for a moment, and the
/// one that keeps a single instance running sends the new one away again —
/// leaving, once this one exits, no Lane at all. Measured: both happened.
///
/// So neither is done here. A small detached shell waits for this process to
/// end, and only then ejects and opens the copy in Applications.
pub fn hand_over(from: &Path, to: &Path) {
    let me = std::process::id();
    let mut script = format!("while /bin/kill -0 {me} 2>/dev/null; do /bin/sleep 0.2; done; ");
    let s = from.to_string_lossy();
    if s.starts_with("/Volumes/") {
        if let Some(vol) = s.split('/').nth(2) {
            script.push_str(&format!("/usr/bin/hdiutil detach '/Volumes/{vol}' -force >/dev/null 2>&1; "));
        }
    }
    script.push_str(&format!("/usr/bin/open -n '{}'", to.display()));
    let _ = Command::new("/bin/sh").arg("-c").arg(&script).spawn();
}

#[cfg(test)]
mod tests {
    use super::wrong_place;
    use std::path::Path;

    #[test]
    fn applications_is_the_right_place() {
        assert!(wrong_place(Path::new("/Applications/Lane.app")).is_none());
        assert!(wrong_place(Path::new("/Applications/Utilities/Lane.app")).is_none());
    }

    #[test]
    fn a_disk_image_and_a_translocated_copy_are_not() {
        assert!(wrong_place(Path::new("/Volumes/Lane/Lane.app")).is_some());
        assert!(wrong_place(Path::new("/private/var/folders/yb/x/T/AppTranslocation/ABC/d/Lane.app")).is_some());
        assert!(wrong_place(Path::new("/Users/p/Downloads/Lane.app")).is_some());
        assert!(wrong_place(Path::new("/Users/p/Desktop/Lane.app")).is_some());
    }

    #[test]
    fn a_build_folder_is_left_alone() {
        assert!(wrong_place(Path::new("/Users/p/code/lane/src-tauri/target/release/bundle/macos/Lane.app")).is_none());
    }
}
