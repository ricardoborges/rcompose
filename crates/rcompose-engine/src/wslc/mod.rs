//! WSLC container engine implementation.

pub mod command;
pub mod discovery;
pub mod paths;
pub mod retry;

use crate::engine::*;
use async_trait::async_trait;
use discovery::find_wslc;
use retry::retry_with_backoff;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Clone, Debug)]
pub struct WslcEngine {
    bin_path: PathBuf,
}

impl WslcEngine {
    pub fn new() -> Result<Self, EngineError> {
        let bin_path = find_wslc().map_err(EngineError::NotFound)?;
        Ok(Self { bin_path })
    }

    pub fn with_binary(bin_path: PathBuf) -> Self {
        Self { bin_path }
    }

    async fn run_cmd(&self, args: &[String]) -> Result<std::process::Output, EngineError> {
        let mut cmd = Command::new(&self.bin_path);
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(EngineError::ExecutionFailed(format!(
                "wslc {} failed (exit {}): {}",
                args.first().cloned().unwrap_or_default(),
                output.status.code().unwrap_or(-1),
                stderr
            )));
        }
        Ok(output)
    }

    async fn run_cmd_json(&self, args: &[String]) -> Result<Value, EngineError> {
        let output = self.run_cmd(args).await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(Value::Array(Vec::new()));
        }
        let val: Value = serde_json::from_str(trimmed)?;
        Ok(val)
    }
}

#[async_trait]
impl ContainerEngine for WslcEngine {
    async fn ping(&self) -> Result<(), EngineError> {
        let args = vec!["version".to_string()];
        self.run_cmd(&args).await?;
        Ok(())
    }

    async fn list_containers(&self, project: &str) -> Result<Vec<ContainerSummary>, EngineError> {
        let args = vec![
            "list".to_string(),
            "-a".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "-f".to_string(),
            format!("label=com.docker.compose.project={}", project),
        ];

        let json_val = self.run_cmd_json(&args).await.unwrap_or(Value::Array(Vec::new()));
        let mut summaries = Vec::new();

        if let Value::Array(items) = json_val {
            for item in items {
                let id = item.get("Id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let name = item.get("Name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let image = item.get("Image").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let status = item.get("Status").and_then(|v| v.as_str()).unwrap_or_default().to_string();

                let mut labels = std::collections::HashMap::new();
                if let Some(Value::Object(map)) = item.get("Labels") {
                    for (k, v) in map {
                        if let Some(s) = v.as_str() {
                            labels.insert(k.clone(), s.to_string());
                        }
                    }
                }

                let service = labels
                    .get("com.docker.compose.service")
                    .cloned()
                    .unwrap_or_default();

                summaries.push(ContainerSummary {
                    id,
                    name,
                    image,
                    service,
                    status,
                    ports: Vec::new(),
                    labels,
                });
            }
        }

        Ok(summaries)
    }

    async fn inspect_container(&self, id_or_name: &str) -> Result<Option<ContainerDetails>, EngineError> {
        let args = vec!["inspect".to_string(), id_or_name.to_string()];
        let json_val = match self.run_cmd_json(&args).await {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        let target = if let Value::Array(ref arr) = json_val {
            arr.first().cloned()
        } else {
            Some(json_val)
        };

        if let Some(item) = target {
            let id = item.get("Id").and_then(|v| v.as_str()).unwrap_or(id_or_name).to_string();
            let name = item.get("Name").and_then(|v| v.as_str()).unwrap_or(id_or_name).to_string();
            let image = item.get("Image").and_then(|v| v.as_str()).unwrap_or_default().to_string();

            let mut labels = std::collections::HashMap::new();
            if let Some(Value::Object(map)) = item.get("Labels") {
                for (k, v) in map {
                    if let Some(s) = v.as_str() {
                        labels.insert(k.clone(), s.to_string());
                    }
                }
            }

            let (state_str, running) = if let Some(state_obj) = item.get("State") {
                let status = state_obj.get("Status").and_then(|v| v.as_str()).unwrap_or("unknown");
                let running = state_obj.get("Running").and_then(|v| v.as_bool()).unwrap_or(status == "running");
                (status.to_string(), running)
            } else {
                ("unknown".to_string(), false)
            };

            let config_hash = labels.get("com.docker.compose.config-hash").cloned();

            Ok(Some(ContainerDetails {
                id,
                name,
                image,
                state: state_str,
                running,
                labels,
                config_hash,
            }))
        } else {
            Ok(None)
        }
    }

    async fn run_container(&self, opts: RunOptions) -> Result<String, EngineError> {
        let args = command::build_run_args(&opts);
        let name = opts.name.clone();

        retry_with_backoff("wslc run", 5, 1000, || async {
            let output = self.run_cmd(&args).await?;
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if stdout.is_empty() {
                Ok(name.clone())
            } else {
                Ok(stdout)
            }
        })
        .await
    }

    async fn start_container(&self, id_or_name: &str) -> Result<(), EngineError> {
        let args = vec!["start".to_string(), id_or_name.to_string()];
        retry_with_backoff("wslc start", 5, 1000, || async {
            self.run_cmd(&args).await?;
            Ok(())
        })
        .await
    }

    async fn stop_container(&self, id_or_name: &str, _timeout_secs: u32) -> Result<(), EngineError> {
        let args = vec!["stop".to_string(), id_or_name.to_string()];
        retry_with_backoff("wslc stop", 5, 1000, || async {
            self.run_cmd(&args).await?;
            Ok(())
        })
        .await
    }

    async fn remove_container(&self, id_or_name: &str, force: bool) -> Result<(), EngineError> {
        let mut args = vec!["remove".to_string()];
        if force {
            args.push("-f".to_string());
        }
        args.push(id_or_name.to_string());

        retry_with_backoff("wslc remove", 5, 1000, || async {
            self.run_cmd(&args).await?;
            Ok(())
        })
        .await
    }

    async fn create_network(&self, name: &str) -> Result<(), EngineError> {
        let args = vec!["network".to_string(), "create".to_string(), name.to_string()];
        self.run_cmd(&args).await?;
        Ok(())
    }

    async fn list_networks(&self) -> Result<Vec<String>, EngineError> {
        let args = vec!["network".to_string(), "list".to_string()];
        let output = self.run_cmd(&args).await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut names = Vec::new();
        for line in stdout.lines().skip(1) {
            if let Some(first_col) = line.split_whitespace().next() {
                names.push(first_col.to_string());
            }
        }
        Ok(names)
    }

    async fn create_volume(&self, name: &str) -> Result<(), EngineError> {
        let args = vec!["volume".to_string(), "create".to_string(), name.to_string()];
        self.run_cmd(&args).await?;
        Ok(())
    }

    async fn list_volumes(&self) -> Result<Vec<String>, EngineError> {
        let args = vec!["volume".to_string(), "list".to_string()];
        let output = self.run_cmd(&args).await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut names = Vec::new();
        for line in stdout.lines().skip(1) {
            if let Some(first_col) = line.split_whitespace().next() {
                names.push(first_col.to_string());
            }
        }
        Ok(names)
    }

    async fn build_image(&self, opts: BuildOptions) -> Result<(), EngineError> {
        let args = command::build_build_args(&opts);
        self.run_cmd(&args).await?;
        Ok(())
    }

    async fn image_exists(&self, image: &str) -> Result<bool, EngineError> {
        let args = vec!["images".to_string(), "--format".to_string(), "json".to_string()];
        let json_val = match self.run_cmd_json(&args).await {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };

        if let Value::Array(items) = json_val {
            for item in items {
                let repo = item.get("Repository").and_then(|v| v.as_str()).unwrap_or_default();
                let tag = item.get("Tag").and_then(|v| v.as_str()).unwrap_or_default();
                let full = format!("{}:{}", repo, tag);
                if repo == image || full == image {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
