//! Path conversion utilities between Windows and WSL.

use std::path::{Path, PathBuf};

pub fn to_host_path(path: &str) -> String {
    // If it already looks like a Windows path (C:\ or C:/ or \\)
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        return path.to_string();
    }
    if path.starts_with(r"\\") {
        return path.to_string();
    }

    // If running under WSL and path is /mnt/c/...
    if path.starts_with("/mnt/") && path.len() >= 7 {
        let drive = &path[5..6];
        let rest = &path[6..];
        return format!("{}:{}", drive.to_uppercase(), rest.replace('/', "\\"));
    }

    // If relative path, try canonicalizing or returning as is
    if let Ok(canon) = std::fs::canonicalize(Path::new(path)) {
        let s = canon.to_string_lossy().to_string();
        // Strip \\?\ Windows prefix if present
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return stripped.to_string();
        }
        return s;
    }

    path.to_string()
}

pub fn make_absolute_if_relative(path: &str, base: &Path) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}
