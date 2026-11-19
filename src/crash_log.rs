use std::backtrace::Backtrace;
use std::fs::OpenOptions;
use std::io::Write;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 256 * 1024;

pub fn log_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("roxel").join("crash.log"))
}

/// Chains onto the existing hook, so the default stderr report still prints.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // A panic inside the hook while already panicking aborts the process
        // before `previous` can print the stderr report.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| log_panic(info)));
        previous(info);
    }));
}

fn log_panic(info: &PanicHookInfo) {
    let Some(path) = log_path() else { return };
    let report = format_report(
        &unix_to_utc(now_secs()),
        &panic_message(info),
        &panic_location(info),
        &Backtrace::force_capture().to_string(),
    );
    let _ = append_report(&path, &report, MAX_LOG_BYTES);
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn panic_message(info: &PanicHookInfo) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn panic_location(info: &PanicHookInfo) -> String {
    info.location()
        .map_or_else(|| "unknown location".to_string(), |l| l.to_string())
}

pub fn format_report(timestamp: &str, message: &str, location: &str, backtrace: &str) -> String {
    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("unnamed");
    format!(
        "=== Roxel {} panicked at {timestamp} ===\nos: {} {}\nthread: {thread}\nat: {location}\n{message}\n\nbacktrace:\n{backtrace}\n\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

/// Rotates to `crash.log.old` before the file grows past `max_bytes`, so only
/// the current and previous logs are kept.
pub fn append_report(path: &Path, report: &str, max_bytes: u64) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let len = std::fs::metadata(path).map_or(0, |m| m.len());
    let rotate = len > 0 && len + report.len() as u64 > max_bytes;
    let rotated = rotate && std::fs::rename(path, path.with_extension("log.old")).is_ok();
    let mut options = OpenOptions::new();
    options.create(true);
    if rotate && !rotated {
        options.write(true).truncate(true);
    } else {
        options.append(true);
    }
    options.open(path)?.write_all(report.as_bytes())
}

pub fn unix_to_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Howard Hinnant's days-since-epoch → proleptic Gregorian date.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_log(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("roxel-crash-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("crash.log")
    }

    #[test]
    fn unix_to_utc_formats_known_dates() {
        assert_eq!(unix_to_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(unix_to_utc(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(unix_to_utc(1_795_093_199), "2026-11-19 12:59:59 UTC");
    }

    #[test]
    fn report_includes_version_location_and_message() {
        let r = format_report("T", "boom", "src/x.rs:1:2", "frames");
        assert!(r.contains(env!("CARGO_PKG_VERSION")));
        assert!(r.contains("panicked at T"));
        assert!(r.contains("at: src/x.rs:1:2\nboom\n"));
        assert!(r.contains("backtrace:\nframes"));
    }

    #[test]
    fn append_creates_dir_and_appends() {
        let path = temp_log("append");
        append_report(&path, "one\n", 1024).unwrap();
        append_report(&path, "two\n", 1024).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn failed_rotation_still_writes_the_report() {
        let path = temp_log("rotate-fails");
        std::fs::create_dir_all(path.with_extension("log.old")).unwrap();
        append_report(&path, "first\n", 10).unwrap();
        append_report(&path, "second\n", 10).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("second\n"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn append_rotates_when_over_limit() {
        let path = temp_log("rotate");
        append_report(&path, "first\n", 10).unwrap();
        append_report(&path, "second\n", 10).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second\n");
        let old = path.with_extension("log.old");
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "first\n");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn oversized_first_report_is_still_written() {
        let path = temp_log("oversized");
        append_report(&path, "a long report\n", 4).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a long report\n");
        assert!(!path.with_extension("log.old").exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
