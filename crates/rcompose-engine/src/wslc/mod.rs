//! WSLC container engine implementation.

pub mod command;
pub mod discovery;
pub mod retry;

use crate::engine::*;
use async_trait::async_trait;
use discovery::find_wslc;
use rcompose_spec::model::{LABEL_CONFIG_HASH, LABEL_INDEX, LABEL_SERVICE};
use retry::retry_with_backoff;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
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

    pub fn binary(&self) -> &Path {
        &self.bin_path
    }

    async fn run_cmd(&self, args: &[String]) -> Result<std::process::Output, EngineError> {
        let output = Command::new(&self.bin_path)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if stderr.is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            } else {
                stderr
            };
            return Err(EngineError::ExecutionFailed(format!(
                "wslc {} failed (exit {}): {}",
                args.iter().take(2).cloned().collect::<Vec<_>>().join(" "),
                output.status.code().unwrap_or(-1),
                detail
            )));
        }
        Ok(output)
    }

    /// Runs a command with stdout/stderr attached to the terminal (builds, pulls).
    async fn run_attached(&self, args: &[String]) -> Result<(), EngineError> {
        let status = Command::new(&self.bin_path)
            .args(args)
            .stdin(Stdio::null())
            .status()
            .await?;
        if !status.success() {
            return Err(EngineError::ExecutionFailed(format!(
                "wslc {} failed (exit {})",
                args.first().cloned().unwrap_or_default(),
                status.code().unwrap_or(-1)
            )));
        }
        Ok(())
    }

    async fn run_retried(&self, op: &str, args: Vec<String>) -> Result<std::process::Output, EngineError> {
        retry_with_backoff(op, 5, 1000, || self.run_cmd(&args)).await
    }

    /// Parses `--format json` output, which wslc emits as one object per line.
    async fn run_json(&self, args: &[String]) -> Result<Vec<Value>, EngineError> {
        let output = self.run_cmd(args).await?;
        parse_json_stream(&String::from_utf8_lossy(&output.stdout))
    }

    async fn names_from_list(&self, args: &[&str]) -> Result<Vec<String>, EngineError> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        Ok(self
            .run_json(&args)
            .await?
            .iter()
            .filter_map(|item| item.get("Name").and_then(Value::as_str).map(String::from))
            .collect())
    }

    async fn inspect_many(&self, ids: &[String]) -> Result<Vec<ContainerDetails>, EngineError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut args = vec!["inspect".to_string()];
        args.extend(ids.iter().cloned());
        Ok(self.run_json(&args).await?.iter().map(parse_container).collect())
    }
}

/// Accepts both a JSON array and newline-delimited JSON objects.
fn parse_json_stream(text: &str) -> Result<Vec<Value>, EngineError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        return match serde_json::from_str(trimmed)? {
            Value::Array(items) => Ok(items),
            other => Ok(vec![other]),
        };
    }
    trimmed
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(EngineError::from))
        .collect()
}

/// Builds container details from one `wslc inspect` object.
fn parse_container(item: &Value) -> ContainerDetails {
    let str_at = |path: &[&str]| -> Option<String> {
        let mut v = item;
        for key in path {
            v = v.get(key)?;
        }
        v.as_str().map(String::from)
    };

    let labels: HashMap<String, String> = item
        .pointer("/Config/Labels")
        .or_else(|| item.get("Labels"))
        .and_then(Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
        .unwrap_or_default();

    let state = item.get("State");
    let status = state.and_then(|s| s.get("Status")).and_then(Value::as_str).unwrap_or("unknown");
    let running = state
        .and_then(|s| s.get("Running"))
        .and_then(Value::as_bool)
        .unwrap_or(status == "running");

    let mut ports = Vec::new();
    let port_map = item.get("Ports").or_else(|| item.pointer("/NetworkSettings/Ports"));
    if let Some(Value::Object(map)) = port_map {
        for (container_port, bindings) in map {
            for b in bindings.as_array().into_iter().flatten() {
                let ip = b.get("HostIp").and_then(Value::as_str).unwrap_or("0.0.0.0");
                let port = b.get("HostPort").and_then(Value::as_str).unwrap_or_default();
                ports.push(format!("{}:{}->{}", ip, port, container_port));
            }
        }
    }
    ports.sort();

    ContainerDetails {
        id: str_at(&["Id"]).unwrap_or_default(),
        name: str_at(&["Name"]).unwrap_or_default().trim_start_matches('/').to_string(),
        image: str_at(&["Config", "Image"]).or_else(|| str_at(&["Image"])).unwrap_or_default(),
        service: labels.get(LABEL_SERVICE).cloned().unwrap_or_default(),
        number: labels.get(LABEL_INDEX).and_then(|n| n.parse().ok()).unwrap_or(1),
        state: status.to_string(),
        running,
        health: state
            .and_then(|s| s.pointer("/Health/Status"))
            .and_then(Value::as_str)
            .map(String::from),
        exit_code: state.and_then(|s| s.get("ExitCode")).and_then(Value::as_i64),
        ports,
        config_hash: labels.get(LABEL_CONFIG_HASH).cloned(),
        labels,
    }
}

fn label_args(labels: &BTreeMap<String, String>) -> Vec<String> {
    labels.iter().flat_map(|(k, v)| ["-l".to_string(), format!("{}={}", k, v)]).collect()
}

#[async_trait]
impl ContainerEngine for WslcEngine {
    async fn ping(&self) -> Result<(), EngineError> {
        self.run_cmd(&["version".to_string()]).await?;
        Ok(())
    }

    async fn list_containers(&self, project: &str) -> Result<Vec<ContainerDetails>, EngineError> {
        let args = vec![
            "list".to_string(),
            "-a".to_string(),
            "--no-trunc".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "-f".to_string(),
            format!("label=com.docker.compose.project={}", project),
        ];
        let ids: Vec<String> = self
            .run_json(&args)
            .await?
            .iter()
            .filter_map(|item| item.get("ID").or_else(|| item.get("Id")).and_then(Value::as_str).map(String::from))
            .collect();
        self.inspect_many(&ids).await
    }

    async fn inspect_container(&self, id_or_name: &str) -> Result<Option<ContainerDetails>, EngineError> {
        match self.inspect_many(&[id_or_name.to_string()]).await {
            Ok(mut found) => Ok(found.pop()),
            Err(err) => {
                // a missing object fails `inspect`: only report the error if the container exists
                let all = self.run_json(&["list".into(), "-a".into(), "--format".into(), "json".into()]).await?;
                let exists = all.iter().any(|c| {
                    c.get("Names").and_then(Value::as_str) == Some(id_or_name)
                        || c.get("ID").and_then(Value::as_str).is_some_and(|id| id_or_name.starts_with(id))
                });
                if exists {
                    Err(err)
                } else {
                    Ok(None)
                }
            }
        }
    }

    async fn create_container(&self, opts: CreateOptions) -> Result<String, EngineError> {
        let output = self.run_retried("wslc create", command::build_create_args(&opts)).await?;
        let id = String::from_utf8_lossy(&output.stdout).lines().last().unwrap_or_default().trim().to_string();
        Ok(if id.is_empty() { opts.name } else { id })
    }

    async fn start_container(&self, id_or_name: &str) -> Result<(), EngineError> {
        self.run_retried("wslc start", vec!["start".into(), id_or_name.into()]).await?;
        Ok(())
    }

    async fn stop_container(&self, id_or_name: &str, timeout_secs: Option<u32>) -> Result<(), EngineError> {
        let mut args = vec!["stop".to_string()];
        if let Some(t) = timeout_secs {
            args.push("-t".to_string());
            args.push(t.to_string());
        }
        args.push(id_or_name.to_string());
        self.run_retried("wslc stop", args).await?;
        Ok(())
    }

    async fn remove_container(&self, id_or_name: &str, force: bool) -> Result<(), EngineError> {
        let mut args = vec!["remove".to_string()];
        if force {
            args.push("-f".to_string());
        }
        args.push(id_or_name.to_string());
        self.run_retried("wslc remove", args).await?;
        Ok(())
    }

    async fn connect_network(&self, network: &str, container: &str, aliases: &[String]) -> Result<(), EngineError> {
        let mut args = vec!["network".to_string(), "connect".to_string()];
        for alias in aliases {
            args.push("--network-alias".to_string());
            args.push(alias.clone());
        }
        args.push(network.to_string());
        args.push(container.to_string());
        self.run_retried("wslc network connect", args).await?;
        Ok(())
    }

    async fn create_network(&self, name: &str, labels: &BTreeMap<String, String>) -> Result<(), EngineError> {
        let mut args = vec!["network".to_string(), "create".to_string()];
        args.extend(label_args(labels));
        args.push(name.to_string());
        self.run_cmd(&args).await?;
        Ok(())
    }

    async fn remove_network(&self, name: &str) -> Result<(), EngineError> {
        self.run_cmd(&["network".into(), "remove".into(), name.into()]).await?;
        Ok(())
    }

    async fn list_networks(&self) -> Result<Vec<String>, EngineError> {
        self.names_from_list(&["network", "list", "--format", "json"]).await
    }

    async fn create_volume(&self, name: &str, labels: &BTreeMap<String, String>) -> Result<(), EngineError> {
        let mut args = vec!["volume".to_string(), "create".to_string()];
        args.extend(label_args(labels));
        args.push(name.to_string());
        self.run_cmd(&args).await?;
        Ok(())
    }

    async fn remove_volume(&self, name: &str) -> Result<(), EngineError> {
        self.run_cmd(&["volume".into(), "remove".into(), name.into()]).await?;
        Ok(())
    }

    async fn list_volumes(&self) -> Result<Vec<String>, EngineError> {
        self.names_from_list(&["volume", "list", "--format", "json"]).await
    }

    async fn build_image(&self, opts: BuildOptions) -> Result<(), EngineError> {
        self.run_attached(&command::build_build_args(&opts)).await
    }

    async fn pull_image(&self, image: &str) -> Result<(), EngineError> {
        self.run_cmd(&["pull".into(), image.into()]).await?;
        Ok(())
    }

    async fn image_exists(&self, image: &str) -> Result<bool, EngineError> {
        let wanted = normalize_image_ref(image);
        let images = self.run_json(&["images".into(), "--format".into(), "json".into()]).await?;
        Ok(images.iter().any(|item| {
            let repo = item.get("Repository").and_then(Value::as_str).unwrap_or_default();
            let tag = item.get("Tag").and_then(Value::as_str).unwrap_or_default();
            normalize_image_ref(&format!("{}:{}", repo, tag)) == wanted
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ndjson_and_arrays() {
        assert_eq!(parse_json_stream("{\"a\":1}\n{\"a\":2}\n").unwrap().len(), 2);
        assert_eq!(parse_json_stream("[{\"a\":1},{\"a\":2}]").unwrap().len(), 2);
        assert!(parse_json_stream("  ").unwrap().is_empty());
    }

    #[test]
    fn parses_inspect_output() {
        let item: Value = serde_json::from_str(
            r#"{
              "Id": "fe3b", "Name": "/proj-web-1", "Image": "sha256:bf85",
              "Config": {"Image": "alpine:3.20", "Labels": {
                "com.docker.compose.service": "web",
                "com.docker.compose.container-number": "2",
                "com.docker.compose.config-hash": "abc"}},
              "Ports": {"80/tcp": [{"HostIp": "127.0.0.1", "HostPort": "18080"}]},
              "State": {"Status": "running", "Running": true, "ExitCode": 0,
                        "Health": {"Status": "healthy"}}
            }"#,
        )
        .unwrap();
        let c = parse_container(&item);
        assert_eq!(c.name, "proj-web-1");
        assert_eq!(c.image, "alpine:3.20");
        assert_eq!(c.service, "web");
        assert_eq!(c.number, 2);
        assert_eq!(c.config_hash.as_deref(), Some("abc"));
        assert_eq!(c.health.as_deref(), Some("healthy"));
        assert!(c.running);
        assert_eq!(c.ports, vec!["127.0.0.1:18080->80/tcp"]);
    }
}
