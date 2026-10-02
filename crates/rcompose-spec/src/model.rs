//! Normalized in-memory model of a Compose project and its services.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use thiserror::Error;

pub const LABEL_PROJECT: &str = "com.docker.compose.project";
pub const LABEL_SERVICE: &str = "com.docker.compose.service";
pub const LABEL_INDEX: &str = "com.docker.compose.container-number";
pub const LABEL_CONFIG_HASH: &str = "com.docker.compose.config-hash";

#[derive(Error, Debug)]
pub enum ModelError {
    #[error("invalid port mapping: '{0}'")]
    InvalidPort(String),
    #[error("invalid volume specification: '{0}'")]
    InvalidVolume(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildConfig {
    pub context: String,
    pub dockerfile: Option<String>,
    #[serde(default)]
    pub args: HashMap<String, String>,
    pub target: Option<String>,
    #[serde(default)]
    pub pull: bool,
    #[serde(default)]
    pub no_cache: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VolumeType {
    Bind,
    Volume,
    Tmpfs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeMount {
    pub mount_type: VolumeType,
    pub source: Option<String>,
    pub target: String,
    #[serde(default)]
    pub read_only: bool,
}

impl VolumeMount {
    pub fn parse(spec: &str) -> Result<Self, ModelError> {
        let parts: Vec<&str> = spec.split(':').collect();
        match parts.len() {
            1 => {
                let target = parts[0].to_string();
                if target.is_empty() {
                    return Err(ModelError::InvalidVolume(spec.to_string()));
                }
                Ok(Self {
                    mount_type: VolumeType::Volume,
                    source: None,
                    target,
                    read_only: false,
                })
            }
            2 => {
                let source = parts[0].to_string();
                let target = parts[1].to_string();
                let mount_type = if source.starts_with('.') || source.starts_with('/') || source.contains('\\') || source.contains(':') {
                    VolumeType::Bind
                } else {
                    VolumeType::Volume
                };
                Ok(Self {
                    mount_type,
                    source: Some(source),
                    target,
                    read_only: false,
                })
            }
            3 => {
                let source = parts[0].to_string();
                let target = parts[1].to_string();
                let read_only = parts[2].split(',').any(|opt| opt == "ro");
                let mount_type = if source.starts_with('.') || source.starts_with('/') || source.contains('\\') {
                    VolumeType::Bind
                } else {
                    VolumeType::Volume
                };
                Ok(Self {
                    mount_type,
                    source: Some(source),
                    target,
                    read_only,
                })
            }
            _ => Err(ModelError::InvalidVolume(spec.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortMapping {
    pub target: u16,
    pub published: Option<String>,
    pub protocol: String,
}

impl PortMapping {
    pub fn parse(spec: &str) -> Result<Self, ModelError> {
        let (spec_clean, protocol) = if let Some((base, proto)) = spec.split_once('/') {
            (base, proto.to_lowercase())
        } else {
            (spec, "tcp".to_string())
        };

        let parts: Vec<&str> = spec_clean.split(':').collect();
        match parts.len() {
            1 => {
                let target = parts[0]
                    .parse::<u16>()
                    .map_err(|_| ModelError::InvalidPort(spec.to_string()))?;
                Ok(Self {
                    target,
                    published: None,
                    protocol,
                })
            }
            2 => {
                let published = parts[0].to_string();
                let target = parts[1]
                    .parse::<u16>()
                    .map_err(|_| ModelError::InvalidPort(spec.to_string()))?;
                Ok(Self {
                    target,
                    published: Some(published),
                    protocol,
                })
            }
            3 => {
                let host_ip = parts[0];
                let host_port = parts[1];
                let target = parts[2]
                    .parse::<u16>()
                    .map_err(|_| ModelError::InvalidPort(spec.to_string()))?;
                Ok(Self {
                    target,
                    published: Some(format!("{}:{}", host_ip, host_port)),
                    protocol,
                })
            }
            _ => Err(ModelError::InvalidPort(spec.to_string())),
        }
    }

    pub fn to_flag(&self) -> String {
        let mut s = String::new();
        if let Some(ref pub_port) = self.published {
            s.push_str(pub_port);
            s.push(':');
        }
        s.push_str(&self.target.to_string());
        if self.protocol != "tcp" {
            s.push('/');
            s.push_str(&self.protocol);
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    #[serde(default)]
    pub name: String,
    pub image: Option<String>,
    pub build: Option<BuildConfig>,
    pub command: Option<Vec<String>>,
    pub entrypoint: Option<Vec<String>>,
    pub container_name: Option<String>,
    #[serde(default)]
    pub environment: HashMap<String, Option<String>>,
    #[serde(default)]
    pub env_files: Vec<String>,
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    #[serde(default)]
    pub tmpfs: Vec<String>,
    #[serde(default)]
    pub networks: Vec<String>,
    #[serde(default)]
    pub network_aliases: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub hostname: Option<String>,
    pub domainname: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dns_search: Vec<String>,
    #[serde(default)]
    pub dns_opt: Vec<String>,
    pub user: Option<String>,
    pub working_dir: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    pub mem_limit: Option<String>,
    pub cpus: Option<String>,
    pub shm_size: Option<String>,
    #[serde(default)]
    pub ulimits: Vec<String>,
    pub stop_signal: Option<String>,
    pub gpus: Option<String>,
    #[serde(default)]
    pub stdin_open: bool,
    #[serde(default)]
    pub tty: bool,
    #[serde(default = "default_replicas")]
    pub replicas: usize,
    #[serde(default)]
    pub profiles: Vec<String>,
    pub restart: Option<String>,
    pub wsl_session: Option<String>,
}

fn default_replicas() -> usize {
    1
}

impl Default for Service {
    fn default() -> Self {
        Self {
            name: String::new(),
            image: None,
            build: None,
            command: None,
            entrypoint: None,
            container_name: None,
            environment: HashMap::new(),
            env_files: Vec::new(),
            ports: Vec::new(),
            volumes: Vec::new(),
            tmpfs: Vec::new(),
            networks: Vec::new(),
            network_aliases: HashMap::new(),
            depends_on: Vec::new(),
            hostname: None,
            domainname: None,
            dns: Vec::new(),
            dns_search: Vec::new(),
            dns_opt: Vec::new(),
            user: None,
            working_dir: None,
            labels: HashMap::new(),
            mem_limit: None,
            cpus: None,
            shm_size: None,
            ulimits: Vec::new(),
            stop_signal: None,
            gpus: None,
            stdin_open: false,
            tty: false,
            replicas: 1,
            profiles: Vec::new(),
            restart: None,
            wsl_session: None,
        }
    }
}

impl Service {
    pub fn config_hash(&self) -> String {
        // Canonical serialization with BTreeMap for key stability
        let mut sorted_env = BTreeMap::new();
        for (k, v) in &self.environment {
            sorted_env.insert(k.clone(), v.clone());
        }
        let mut sorted_labels = BTreeMap::new();
        for (k, v) in &self.labels {
            sorted_labels.insert(k.clone(), v.clone());
        }

        let canonical_repr = serde_json::json!({
            "image": &self.image,
            "build": &self.build,
            "command": &self.command,
            "entrypoint": &self.entrypoint,
            "environment": sorted_env,
            "ports": &self.ports,
            "volumes": &self.volumes,
            "tmpfs": &self.tmpfs,
            "networks": &self.networks,
            "user": &self.user,
            "working_dir": &self.working_dir,
            "labels": sorted_labels,
            "mem_limit": &self.mem_limit,
            "cpus": &self.cpus,
            "shm_size": &self.shm_size,
            "ulimits": &self.ulimits,
            "stop_signal": &self.stop_signal,
            "gpus": &self.gpus,
            "wsl_session": &self.wsl_session,
        });

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        // Also use sha2-compatible or formatted hash
        let json_str = serde_json::to_string(&canonical_repr).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        json_str.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub directory: PathBuf,
    #[serde(default)]
    pub services: HashMap<String, Service>,
    #[serde(default)]
    pub networks: HashMap<String, NetworkConfig>,
    #[serde(default)]
    pub volumes: HashMap<String, VolumeConfig>,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            directory: PathBuf::from("."),
            services: HashMap::new(),
            networks: HashMap::new(),
            volumes: HashMap::new(),
        }
    }

    pub fn container_name(&self, service: &Service, index: usize) -> String {
        if let Some(ref custom) = service.container_name {
            custom.clone()
        } else {
            format!("{}-{}-{}", self.name, service.name, index)
        }
    }
}
