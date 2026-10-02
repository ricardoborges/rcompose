//! Multiplexed streaming log reader with colored prefixes per service.

use colored::{ColoredString, Colorize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const COLORS: &[fn(&str) -> ColoredString] = &[
    |s| s.cyan(),
    |s| s.green(),
    |s| s.yellow(),
    |s| s.blue(),
    |s| s.magenta(),
    |s| s.bright_cyan(),
    |s| s.bright_green(),
    |s| s.bright_yellow(),
];

pub struct LogMultiplexer {
    wslc_bin: PathBuf,
}

impl LogMultiplexer {
    pub fn new(wslc_bin: PathBuf) -> Self {
        Self { wslc_bin }
    }

    pub async fn stream_logs(
        &self,
        containers: &[(String, String)], // (container_name, service_name)
        follow: bool,
        tail: Option<usize>,
    ) -> anyhow::Result<()> {
        let mut color_map = HashMap::new();
        for (i, (_, service)) in containers.iter().enumerate() {
            color_map.entry(service.clone()).or_insert_with(|| {
                let color_fn = COLORS[i % COLORS.len()];
                color_fn
            });
        }

        let mut tasks = Vec::new();

        for (cname, sname) in containers {
            let bin = self.wslc_bin.clone();
            let cname = cname.clone();
            let sname = sname.clone();
            let color_fn = color_map[&sname];

            let handle = tokio::spawn(async move {
                let mut cmd = Command::new(bin);
                cmd.arg("logs");
                if follow {
                    cmd.arg("-f");
                }
                if let Some(t) = tail {
                    cmd.arg("--tail");
                    cmd.arg(t.to_string());
                }
                cmd.arg(&cname);
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());

                if let Ok(mut child) = cmd.spawn() {
                    let stdout = child.stdout.take();
                    let stderr = child.stderr.take();

                    let prefix = format!("{:15} |", sname);
                    let colored_prefix = color_fn(&prefix);

                    if let Some(out) = stdout {
                        let mut reader = BufReader::new(out).lines();
                        let p = colored_prefix.clone();
                        tokio::spawn(async move {
                            while let Ok(Some(line)) = reader.next_line().await {
                                println!("{} {}", p, line);
                            }
                        });
                    }

                    if let Some(err) = stderr {
                        let mut reader = BufReader::new(err).lines();
                        let p = colored_prefix.clone();
                        tokio::spawn(async move {
                            while let Ok(Some(line)) = reader.next_line().await {
                                eprintln!("{} {}", p, line);
                            }
                        });
                    }

                    let _ = child.wait().await;
                }
            });

            tasks.push(handle);
        }

        for task in tasks {
            let _ = task.await;
        }

        Ok(())
    }
}
