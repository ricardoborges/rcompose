//! Terminal output helpers: Compose-style progress lines and status messages.

use colored::Colorize;
use rcompose_core::orchestrator::{ProgressEvent, ProgressKind, Reporter};
use std::sync::Arc;

/// Prints each orchestrator step as ` ✔ Container web-1  Started`.
pub fn progress_reporter() -> Reporter {
    Arc::new(|ev: ProgressEvent| {
        let line = format!("{:<44} {}", ev.resource, ev.status);
        match ev.kind {
            ProgressKind::Working => eprintln!(" {} {}", "⠿".cyan(), line.dimmed()),
            ProgressKind::Done => eprintln!(" {} {}", "✔".green().bold(), line),
            ProgressKind::Warning => eprintln!(" {} {}", "!".yellow().bold(), line.yellow()),
        }
    })
}

pub fn warn(msg: &str) {
    eprintln!("{} {}", "WARN".yellow().bold(), msg.yellow());
}
