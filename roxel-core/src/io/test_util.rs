use std::path::PathBuf;

/// Unique temp-file path for io test fixtures. PID + nanosecond timestamp keep
/// parallel tests from colliding. Caller is responsible for `remove_file`.
pub fn tmp_path(name: &str, ext: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!("roxel-test-{pid}-{nanos}-{name}.{ext}"));
    p
}
