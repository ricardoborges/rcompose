//! Normalized in-memory model of a Compose project and its services.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use thiserror::Error;

pub const LABEL_PROJECT: &str = "com.docker.compose.project";
pub const LABEL_SERVICE: &str = "com.docker.compose.service";
pub const LABEL_INDEX: &str = "com.docker.compose.container-number";
pub const LABEL_CONFIG_HASH: &str = "com.docker.compose.config-hash";
pub const LABEL_NETWORK: &str = "com.docker.compose.network";
pub const LABEL_VOLUME: &str = "com.docker.compose.volume";

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
    pub args: BTreeMap<String, String>,
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

/// True if a volume source names a host path rather than a named volume.
pub fn is_host_path(source: &str) -> bool {
    source.starts_with(['/', '.', '~', '\\']) || is_windows_drive_path(source)
}

/// `C:\...` or `C:/...`
pub fn is_windows_drive_path(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

impl VolumeMount {
    /// Parses the short syntax `[source:]target[:mode]`, keeping Windows drive letters
    /// (`C:\data:/data`) attached to the source.
    pub fn parse(spec: &str) -> Result<Self, ModelError> {
        let mut parts: Vec<String> = Vec::new();
        for piece in spec.split(':') {
            // "C" followed by "\..." or "/..." is a drive letter, not a separator
            let is_drive = parts.len() == 1
                && parts[0].len() == 1
                && parts[0].as_bytes()[0].is_ascii_alphabetic()
                && piece.starts_with(['\\', '/']);
            if is_drive {
                parts[0].push(':');
                parts[0].push_str(piece);
            } else {
                parts.push(piece.to_string());
            }
        }

        let (source, target, mode) = match parts.as_slice() {
            [target] => (None, target.clone(), ""),
            [source, target] => (Some(source.clone()), target.clone(), ""),
            [source, target, mode] => (Some(source.clone()), target.clone(), mode.as_str()),
            _ => return Err(ModelError::InvalidVolume(spec.to_string())),
        };
        if target.is_empty() || source.as_deref() == Some("") {
            return Err(ModelError::InvalidVolume(spec.to_string()));
        }

        let mount_type = match source.as_deref() {
            Some(s) if is_host_path(s) => VolumeType::Bind,
            _ => VolumeType::Volume,
        };
        Ok(Self {
            mount_type,
            source,
            target,
            read_only: mode.split(',').any(|opt| opt == "ro"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortMapping {
    pub target: u16,
    pub published: Option<String>,
    pub protocol: String,
}

impl PortMapping {
    /// Parses `[[ip:]published:]target[/protocol]`, expanding ranges such as
    /// `8000-8001:9000-9001` into one mapping per port.
    pub fn parse(spec: &str) -> Result<Vec<Self>, ModelError> {
        let err = || ModelError::InvalidPort(spec.to_string());
        let (spec_clean, protocol) = match spec.rsplit_once('/') {
            Some((base, proto)) => (base, proto.to_lowercase()),
            None => (spec, "tcp".to_string()),
        };

        let (host_ip, published, target) = match spec_clean.split(':').collect::<Vec<_>>()[..] {
            [target] => (None, None, target),
            [published, target] => (None, Some(published), target),
            [ip, published, target] => (Some(ip), Some(published).filter(|p| !p.is_empty()), target),
            _ => return Err(err()),
        };

        let expand = |range: &str| -> Result<Vec<u16>, ModelError> {
            match range.split_once('-') {
                Some((lo, hi)) => {
                    let lo: u16 = lo.parse().map_err(|_| err())?;
                    let hi: u16 = hi.parse().map_err(|_| err())?;
                    if lo > hi {
                        return Err(err());
                    }
                    Ok((lo..=hi).collect())
                }
                None => Ok(vec![range.parse().map_err(|_| err())?]),
            }
        };

        let targets = expand(target)?;
        let published_ports: Vec<Option<String>> = match published {
            None => vec![None; targets.len()],
            Some(p) => {
                let hosts = expand(p)?;
                if hosts.len() != targets.len() {
                    return Err(err());
                }
                hosts.into_iter().map(|h| Some(h.to_string())).collect()
            }
        };

        Ok(targets
            .into_iter()
            .zip(published_ports)
            .map(|(target, published)| {
                let published = match (host_ip, published) {
                    (Some(ip), Some(p)) => Some(format!("{}:{}", ip, p)),
                    (Some(ip), None) => Some(format!("{}:", ip)),
                    (None, p) => p,
                };
                Self { target, published, protocol: protocol.clone() }
            })
            .collect())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DependencyCondition {
    #[default]
    ServiceStarted,
    ServiceHealthy,
    ServiceCompletedSuccessfully,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Healthcheck {
    /// Normalized test: `["CMD", ...]`, `["CMD-SHELL", "..."]` or `["NONE"]`.
    pub test: Vec<String>,
    pub interval: Option<String>,
    pub timeout: Option<String>,
    pub start_period: Option<String>,
    pub retries: Option<u32>,
    #[serde(default)]
    pub disable: bool,
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
    /// Effective environment: `env_file` contents overlaid by `environment`.
    #[serde(default)]
    pub environment: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    #[serde(default)]
    pub tmpfs: Vec<String>,
    /// Engine-level network names; the first one is the primary network.
    #[serde(default)]
    pub networks: Vec<String>,
    /// Extra aliases per engine-level network name.
    #[serde(default)]
    pub network_aliases: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Condition per dependency; a missing entry means `service_started`.
    #[serde(default)]
    pub dependency_conditions: BTreeMap<String, DependencyCondition>,
    /// Dependencies declared with `required: false`.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub optional_dependencies: BTreeSet<String>,
    pub healthcheck: Option<Healthcheck>,
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
    pub labels: BTreeMap<String, String>,
    pub mem_limit: Option<String>,
    pub cpus: Option<String>,
    pub shm_size: Option<String>,
    #[serde(default)]
    pub ulimits: Vec<String>,
    pub stop_signal: Option<String>,
    pub stop_grace_period: Option<u32>,
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
            environment: BTreeMap::new(),
            ports: Vec::new(),
            volumes: Vec::new(),
            tmpfs: Vec::new(),
            networks: Vec::new(),
            network_aliases: BTreeMap::new(),
            depends_on: Vec::new(),
            dependency_conditions: BTreeMap::new(),
            optional_dependencies: BTreeSet::new(),
            healthcheck: None,
            hostname: None,
            domainname: None,
            dns: Vec::new(),
            dns_search: Vec::new(),
            dns_opt: Vec::new(),
            user: None,
            working_dir: None,
            labels: BTreeMap::new(),
            mem_limit: None,
            cpus: None,
            shm_size: None,
            ulimits: Vec::new(),
            stop_signal: None,
            stop_grace_period: None,
            gpus: None,
            stdin_open: false,
            tty: false,
            replicas: 1,
            profiles: Vec::new(),
            restart: None,
        }
    }
}

impl Service {
    pub fn dependency_condition(&self, dependency: &str) -> DependencyCondition {
        self.dependency_conditions.get(dependency).copied().unwrap_or_default()
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
    pub networks: BTreeMap<String, NetworkConfig>,
    #[serde(default)]
    pub volumes: BTreeMap<String, VolumeConfig>,
    /// Services defined in the file but disabled by inactive profiles.
    #[serde(skip)]
    pub disabled_services: BTreeSet<String>,
    /// Non-fatal problems found while loading (unsupported keys, ignored mounts...).
    #[serde(skip)]
    pub warnings: Vec<String>,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            directory: PathBuf::from("."),
            services: HashMap::new(),
            networks: BTreeMap::new(),
            volumes: BTreeMap::new(),
            disabled_services: BTreeSet::new(),
            warnings: Vec::new(),
        }
    }

    pub fn container_name(&self, service: &Service, index: usize) -> String {
        if let Some(ref custom) = service.container_name {
            custom.clone()
        } else {
            format!("{}-{}-{}", self.name, service.name, index)
        }
    }

    /// Image a service runs: its `image`, or `<project>-<service>` when only `build` is set.
    pub fn image_name(&self, service: &Service) -> String {
        service
            .image
            .clone()
            .unwrap_or_else(|| format!("{}-{}", self.name, service.name))
    }

    /// Name of the network services join when they declare none.
    pub fn default_network_name(&self) -> String {
        format!("{}_default", self.name)
    }
}
