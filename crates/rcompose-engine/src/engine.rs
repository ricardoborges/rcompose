//! Core ContainerEngine trait and data transfer structures.

use async_trait::async_trait;
use rcompose_spec::model::{Healthcheck, PortMapping, VolumeMount};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    ExecutionFailed(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Transient engine error: {0}")]
    Transient(String),
    #[error("JSON decode error: {0}")]
    Json(#[from] serde_json::Error),
}

/// State of a container as reported by the engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContainerDetails {
    pub id: String,
    pub name: String,
    pub image: String,
    /// `com.docker.compose.service` label.
    pub service: String,
    /// `com.docker.compose.container-number` label (replica index).
    pub number: usize,
    /// Engine state, e.g. `running`, `exited`, `created`.
    pub state: String,
    pub running: bool,
    /// Healthcheck status (`starting`, `healthy`, `unhealthy`) when the container has one.
    pub health: Option<String>,
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    pub config_hash: Option<String>,
}

/// Everything needed to create one container of a service.
#[derive(Debug, Clone, Default)]
pub struct CreateOptions {
    pub name: String,
    pub image: String,
    pub command: Option<Vec<String>>,
    pub entrypoint: Option<Vec<String>>,
    pub environment: BTreeMap<String, Option<String>>,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMount>,
    pub tmpfs: Vec<String>,
    /// Primary network, joined at creation; others are attached with `connect_network`.
    pub network: Option<String>,
    pub network_aliases: Vec<String>,
    pub labels: BTreeMap<String, String>,
    pub hostname: Option<String>,
    pub domainname: Option<String>,
    pub dns: Vec<String>,
    pub dns_search: Vec<String>,
    pub dns_opt: Vec<String>,
    pub user: Option<String>,
    pub working_dir: Option<String>,
    pub mem_limit: Option<String>,
    pub cpus: Option<String>,
    pub shm_size: Option<String>,
    pub ulimits: Vec<String>,
    pub stop_signal: Option<String>,
    pub stop_timeout: Option<u32>,
    pub gpus: Option<String>,
    pub healthcheck: Option<Healthcheck>,
    pub stdin_open: bool,
    pub tty: bool,
}

#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    pub tag: String,
    pub context: String,
    pub dockerfile: Option<String>,
    pub args: BTreeMap<String, String>,
    pub target: Option<String>,
    pub pull: bool,
    pub no_cache: bool,
}

#[async_trait]
pub trait ContainerEngine: Send + Sync {
    async fn ping(&self) -> Result<(), EngineError>;
    /// All containers (any state) labeled with the given Compose project.
    async fn list_containers(&self, project: &str) -> Result<Vec<ContainerDetails>, EngineError>;
    /// `Ok(None)` when no container has that name or id.
    async fn inspect_container(&self, id_or_name: &str) -> Result<Option<ContainerDetails>, EngineError>;
    /// Creates (without starting) a container; returns its id.
    async fn create_container(&self, opts: CreateOptions) -> Result<String, EngineError>;
    async fn start_container(&self, id_or_name: &str) -> Result<(), EngineError>;
    async fn stop_container(&self, id_or_name: &str, timeout_secs: Option<u32>) -> Result<(), EngineError>;
    async fn remove_container(&self, id_or_name: &str, force: bool) -> Result<(), EngineError>;
    async fn connect_network(&self, network: &str, container: &str, aliases: &[String]) -> Result<(), EngineError>;
    async fn create_network(&self, name: &str, labels: &BTreeMap<String, String>) -> Result<(), EngineError>;
    async fn remove_network(&self, name: &str) -> Result<(), EngineError>;
    async fn list_networks(&self) -> Result<Vec<String>, EngineError>;
    async fn create_volume(&self, name: &str, labels: &BTreeMap<String, String>) -> Result<(), EngineError>;
    async fn remove_volume(&self, name: &str) -> Result<(), EngineError>;
    async fn list_volumes(&self) -> Result<Vec<String>, EngineError>;
    async fn build_image(&self, opts: BuildOptions) -> Result<(), EngineError>;
    async fn pull_image(&self, image: &str) -> Result<(), EngineError>;
    async fn image_exists(&self, image: &str) -> Result<bool, EngineError>;
}

/// Splits an image reference into a normalized `(repository, tag)` pair:
/// `nginx` → `("nginx", "latest")`, `docker.io/library/redis:7` → `("redis", "7")`.
pub fn normalize_image_ref(image: &str) -> (String, String) {
    let without_digest = image.split('@').next().unwrap_or(image);
    let (repo, tag) = match without_digest.rsplit_once(':') {
        Some((repo, tag)) if !tag.contains('/') => (repo, tag),
        _ => (without_digest, "latest"),
    };
    let repo = repo.strip_prefix("docker.io/").unwrap_or(repo);
    let repo = repo.strip_prefix("library/").unwrap_or(repo);
    (repo.to_string(), tag.to_string())
}
