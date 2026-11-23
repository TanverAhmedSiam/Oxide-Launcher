//! Tiny breadcrumb log (logs/trace.log). If the launcher dies without a Rust panic
//! (a native crash), the last line shows what it was doing.
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn init(path: PathBuf) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&path, b"");
    let _ = PATH.set(path);
}

pub fn log(msg: &str) {
    if let Some(p) = PATH.get() {
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(p) {
            let _ = writeln!(f, "{msg}");
        }
    }
}
