//! Core ContainerEngine trait and data transfer structures.

use async_trait::async_trait;
use rcompose_spec::model::{PortMapping, VolumeMount};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CLI execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Transient engine error: {0}")]
    Transient(String),
    #[error("JSON decode error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSummary {
    pub id: String,
    pub name: String,
    pub image: String,
    pub service: String,
    pub status: String,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerDetails {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub running: bool,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    pub config_hash: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub name: String,
    pub image: String,
    pub detach: bool,
    pub command: Option<Vec<String>>,
    pub entrypoint: Option<Vec<String>>,
    pub environment: HashMap<String, Option<String>>,
    pub env_files: Vec<String>,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMount>,
    pub tmpfs: Vec<String>,
    pub networks: Vec<String>,
    pub network_aliases: HashMap<String, Vec<String>>,
    pub labels: HashMap<String, String>,
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
    pub gpus: Option<String>,
    pub stdin_open: bool,
    pub tty: bool,
    pub wsl_session: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    pub tag: String,
    pub context: String,
    pub dockerfile: Option<String>,
    pub args: HashMap<String, String>,
    pub target: Option<String>,
    pub pull: bool,
    pub no_cache: bool,
}

#[async_trait]
pub trait ContainerEngine: Send + Sync {
    async fn ping(&self) -> Result<(), EngineError>;
    async fn list_containers(&self, project: &str) -> Result<Vec<ContainerSummary>, EngineError>;
    async fn inspect_container(&self, id_or_name: &str) -> Result<Option<ContainerDetails>, EngineError>;
    async fn run_container(&self, opts: RunOptions) -> Result<String, EngineError>;
    async fn start_container(&self, id_or_name: &str) -> Result<(), EngineError>;
    async fn stop_container(&self, id_or_name: &str, timeout_secs: u32) -> Result<(), EngineError>;
    async fn remove_container(&self, id_or_name: &str, force: bool) -> Result<(), EngineError>;
    async fn create_network(&self, name: &str) -> Result<(), EngineError>;
    async fn list_networks(&self) -> Result<Vec<String>, EngineError>;
    async fn create_volume(&self, name: &str) -> Result<(), EngineError>;
    async fn list_volumes(&self) -> Result<Vec<String>, EngineError>;
    async fn build_image(&self, opts: BuildOptions) -> Result<(), EngineError>;
    async fn image_exists(&self, image: &str) -> Result<bool, EngineError>;
}
