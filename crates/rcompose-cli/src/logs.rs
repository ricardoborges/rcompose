//! Multiplexed streaming log reader with colored prefixes per container.

use colored::{ColoredString, Colorize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const COLORS: &[fn(&str) -> ColoredString] = &[
    |s| s.cyan(),
    |s| s.yellow(),
    |s| s.green(),
    |s| s.magenta(),
    |s| s.blue(),
    |s| s.bright_cyan(),
    |s| s.bright_yellow(),
    |s| s.bright_green(),
];

pub struct LogOptions {
    pub follow: bool,
    pub timestamps: bool,
    pub tail: Option<String>,
}

/// Streams `wslc logs` of every container, each line prefixed with the container name.
/// Returns when all streams end; dropping the future kills the child processes.
pub async fn stream_logs(wslc_bin: PathBuf, containers: &[String], opts: LogOptions) -> anyhow::Result<()> {
    let width = containers.iter().map(String::len).max().unwrap_or(0);
    let mut tasks = Vec::new();

    for (i, cname) in containers.iter().enumerate() {
        let mut cmd = Command::new(&wslc_bin);
        cmd.arg("logs");
        if opts.follow {
            cmd.arg("-f");
        }
        if opts.timestamps {
            cmd.arg("-t");
        }
        if let Some(ref t) = opts.tail {
            cmd.args(["-n", t]);
        }
        cmd.arg(cname)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn()?;
        let prefix = COLORS[i % COLORS.len()](&format!("{:<width$} |", cname, width = width)).to_string();

        let mut pumps = Vec::new();
        if let Some(out) = child.stdout.take() {
            let p = prefix.clone();
            pumps.push(tokio::spawn(async move {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    println!("{} {}", p, line);
                }
            }));
        }
        if let Some(err) = child.stderr.take() {
            let p = prefix.clone();
            pumps.push(tokio::spawn(async move {
                let mut lines = BufReader::new(err).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("{} {}", p, line);
                }
            }));
        }

        tasks.push(async move {
            let _ = child.wait().await;
            for pump in pumps {
                let _ = pump.await;
            }
        });
    }

    // children run concurrently; waiting in order just collects them
    for task in tasks {
        task.await;
    }
    Ok(())
}
