//! Diagnostics: a small append-only log file in the app data folder and a
//! panic hook that writes there, so a stalled build can be explained
//! without a debugger. Never contains captured text.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

struct FileLogger {
    path: PathBuf,
    file: Mutex<Option<std::fs::File>>,
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) || !record.target().starts_with("rat_mac") {
            return;
        }
        let line = format!("{} {:5} {}\n", now_iso(), record.level(), record.args());
        if let Ok(mut guard) = self.file.lock() {
            if guard.is_none() {
                *guard = OpenOptions::new().create(true).append(true).open(&self.path).ok();
            }
            if let Some(f) = guard.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
        }
        eprint!("{line}");
    }

    fn flush(&self) {}
}

fn now_iso() -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let s = ms / 1000;
    let (days, rem) = (s / 86_400, s % 86_400);
    // Civil date from days since epoch (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

pub fn init(path: &Path) {
    // Keep the file from growing without bound: rotate at ~2 MB.
    if std::fs::metadata(path).map(|m| m.len() > 2_000_000).unwrap_or(false) {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
    let logger = Box::leak(Box::new(FileLogger { path: path.to_path_buf(), file: Mutex::new(None) }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        log::error!("panic on thread {thread}: {info}");
    }));
}

#[cfg(test)]
mod tests {
    #[test]
    fn iso_timestamp_shape() {
        let t = super::now_iso();
        assert_eq!(t.len(), 20, "{t}");
        assert!(t.starts_with("20") && t.ends_with('Z'));
    }
}
