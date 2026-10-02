//! Terminal UI helpers, progress spinners, and status output.

use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

pub fn spinner(message: &'static str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏✔")
            .template("{spinner:.green} {msg}")
            .expect("valid template"),
    );
    pb.set_message(message);
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

pub fn success(msg: &str) {
    println!("{} {}", "✔".green().bold(), msg);
}

pub fn info(msg: &str) {
    println!("{} {}", "ℹ".blue().bold(), msg);
}

#[allow(dead_code)]
pub fn warn(msg: &str) {
    eprintln!("{} {}", "⚠".yellow().bold(), msg.yellow());
}

#[allow(dead_code)]
pub fn error(msg: &str) {
    eprintln!("{} {}", "✖".red().bold(), msg.red());
}
