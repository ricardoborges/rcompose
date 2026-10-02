//! Discovery logic for finding the wslc.exe binary.

use std::env;
use std::path::{Path, PathBuf};

const WSLC_FALLBACKS: &[&str] = &[
    "C:\\Program Files\\WSL\\wslc.exe",
    "/mnt/c/Program Files/WSL/wslc.exe",
];

pub fn find_wslc() -> Result<PathBuf, String> {
    if let Ok(override_bin) = env::var("WSLC_BIN") {
        let p = PathBuf::from(override_bin);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Ok(override_bin) = env::var("WSLC_COMPOSE_BIN") {
        let p = PathBuf::from(override_bin);
        if p.exists() {
            return Ok(p);
        }
    }

    if let Ok(path) = which::which("wslc.exe") {
        return Ok(path);
    }
    if let Ok(path) = which::which("wslc") {
        return Ok(path);
    }

    for fallback in WSLC_FALLBACKS {
        let p = Path::new(fallback);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }

    Err("wslc binary not found. Ensure the WSL container preview is installed (https://learn.microsoft.com/windows/wsl/wsl-container) or set WSLC_BIN.".to_string())
}
