//! Compose file loading, parsing, and merging with rcompose.yml extensions.

use crate::extension::RcomposeFile;
use crate::interpolation::{build_effective_env, interpolate_string};
use crate::model::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LoaderError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Interpolation error: {0}")]
    Interpolation(#[from] crate::interpolation::InterpolationError),
    #[error("Compose file not found: {0}")]
    FileNotFound(String),
    #[error("Invalid Compose structure: {0}")]
    InvalidStructure(String),
}

#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    pub project_name: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub env_file: Option<PathBuf>,
    pub profiles: Vec<String>,
}

const COMPOSE_FILENAMES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

const RCOMPOSE_FILENAMES: &[&str] = &[
    "rcompose.yaml",
    "rcompose.yml",
];

pub fn find_compose_file(dir: &Path) -> Option<PathBuf> {
    let mut current = dir.to_path_buf();
    loop {
        for filename in COMPOSE_FILENAMES {
            let candidate = current.join(filename);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if !current.pop() {
            break;
        }
    }
    None
}

pub fn find_rcompose_extension_file(dir: &Path) -> Option<PathBuf> {
    for filename in RCOMPOSE_FILENAMES {
        let candidate = dir.join(filename);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawComposeFile {
    pub version: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub services: HashMap<String, RawService>,
    #[serde(default)]
    pub networks: HashMap<String, Option<RawNetwork>>,
    #[serde(default)]
    pub volumes: HashMap<String, Option<RawVolume>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawService {
    pub image: Option<String>,
    pub build: Option<RawBuild>,
    pub command: Option<RawStringOrList>,
    pub entrypoint: Option<RawStringOrList>,
    pub container_name: Option<String>,
    pub environment: Option<RawEnvironment>,
    #[serde(default)]
    pub env_file: Option<RawStringOrList>,
    #[serde(default)]
    pub ports: Vec<serde_yaml::Value>,
    #[serde(default)]
    pub volumes: Vec<serde_yaml::Value>,
    #[serde(default)]
    pub tmpfs: Option<RawStringOrList>,
    pub networks: Option<RawNetworks>,
    pub depends_on: Option<RawDependsOn>,
    pub hostname: Option<String>,
    pub domainname: Option<String>,
    pub dns: Option<RawStringOrList>,
    pub dns_search: Option<RawStringOrList>,
    pub dns_opt: Option<RawStringOrList>,
    pub user: Option<String>,
    pub working_dir: Option<String>,
    #[serde(default)]
    pub labels: Option<RawLabels>,
    pub mem_limit: Option<String>,
    pub cpus: Option<serde_yaml::Value>,
    pub shm_size: Option<String>,
    #[serde(default)]
    pub ulimits: HashMap<String, serde_yaml::Value>,
    pub stop_signal: Option<String>,
    pub gpus: Option<String>,
    #[serde(default)]
    pub stdin_open: bool,
    #[serde(default)]
    pub tty: bool,
    pub replicas: Option<usize>,
    #[serde(default)]
    pub profiles: Vec<String>,
    pub restart: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawBuild {
    Simple(String),
    Detailed {
        context: String,
        dockerfile: Option<String>,
        #[serde(default)]
        args: HashMap<String, String>,
        target: Option<String>,
        #[serde(default)]
        pull: bool,
        #[serde(default)]
        no_cache: bool,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawStringOrList {
    Single(String),
    List(Vec<String>),
}

impl RawStringOrList {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::Single(s) => vec![s],
            Self::List(l) => l,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawEnvironment {
    List(Vec<String>),
    Map(HashMap<String, Option<serde_yaml::Value>>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawLabels {
    List(Vec<String>),
    Map(HashMap<String, String>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawNetworks {
    List(Vec<String>),
    Map(HashMap<String, Option<serde_yaml::Value>>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawDependsOn {
    List(Vec<String>),
    Map(HashMap<String, serde_yaml::Value>),
}

#[derive(Debug, Deserialize)]
struct RawNetwork {
    pub name: Option<String>,
    #[serde(default)]
    pub external: bool,
}

#[derive(Debug, Deserialize)]
struct RawVolume {
    pub name: Option<String>,
    #[serde(default)]
    pub external: bool,
}

pub fn load_project_from_str(
    compose_content: &str,
    rcompose_content: Option<&str>,
    opts: LoadOptions,
) -> Result<Project, LoaderError> {
    let env_map = if let Some(e) = opts.env {
        e
    } else {
        build_effective_env(opts.env_file.as_deref())
    };

    let interpolated_compose = interpolate_string(compose_content, &env_map)?;
    let raw_compose: RawComposeFile = serde_yaml::from_str(&interpolated_compose)?;

    let project_name = opts
        .project_name
        .or(raw_compose.name)
        .unwrap_or_else(|| "default".to_string());

    let mut project = Project::new(project_name);

    for (svc_name, raw_svc) in raw_compose.services {
        let mut service = Service::default();
        service.name = svc_name.clone();
        service.image = raw_svc.image;

        if let Some(raw_build) = raw_svc.build {
            service.build = Some(match raw_build {
                RawBuild::Simple(context) => BuildConfig {
                    context,
                    dockerfile: None,
                    args: HashMap::new(),
                    target: None,
                    pull: false,
                    no_cache: false,
                },
                RawBuild::Detailed {
                    context,
                    dockerfile,
                    args,
                    target,
                    pull,
                    no_cache,
                } => BuildConfig {
                    context,
                    dockerfile,
                    args,
                    target,
                    pull,
                    no_cache,
                },
            });
        }

        service.command = raw_svc.command.map(|c| c.into_vec());
        service.entrypoint = raw_svc.entrypoint.map(|e| e.into_vec());
        service.container_name = raw_svc.container_name;

        if let Some(raw_env) = raw_svc.environment {
            match raw_env {
                RawEnvironment::List(list) => {
                    for item in list {
                        if let Some((k, v)) = item.split_once('=') {
                            service.environment.insert(k.to_string(), Some(v.to_string()));
                        } else {
                            service.environment.insert(item, None);
                        }
                    }
                }
                RawEnvironment::Map(map) => {
                    for (k, v) in map {
                        let str_val = v.map(|val| match val {
                            serde_yaml::Value::String(s) => s,
                            serde_yaml::Value::Number(n) => n.to_string(),
                            serde_yaml::Value::Bool(b) => b.to_string(),
                            other => format!("{:?}", other),
                        });
                        service.environment.insert(k, str_val);
                    }
                }
            }
        }

        if let Some(ef) = raw_svc.env_file {
            service.env_files = ef.into_vec();
        }

        for port_val in raw_svc.ports {
            let spec_str = match port_val {
                serde_yaml::Value::String(s) => s,
                serde_yaml::Value::Number(n) => n.to_string(),
                _ => continue,
            };
            if let Ok(pm) = PortMapping::parse(&spec_str) {
                service.ports.push(pm);
            }
        }

        for vol_val in raw_svc.volumes {
            let spec_str = match vol_val {
                serde_yaml::Value::String(s) => s,
                _ => continue,
            };
            if let Ok(vm) = VolumeMount::parse(&spec_str) {
                service.volumes.push(vm);
            }
        }

        if let Some(t) = raw_svc.tmpfs {
            service.tmpfs = t.into_vec();
        }

        if let Some(networks) = raw_svc.networks {
            match networks {
                RawNetworks::List(list) => service.networks = list,
                RawNetworks::Map(map) => service.networks = map.into_keys().collect(),
            }
        }

        if let Some(depends) = raw_svc.depends_on {
            match depends {
                RawDependsOn::List(list) => service.depends_on = list,
                RawDependsOn::Map(map) => service.depends_on = map.into_keys().collect(),
            }
        }

        service.hostname = raw_svc.hostname;
        service.domainname = raw_svc.domainname;
        service.dns = raw_svc.dns.map(|d| d.into_vec()).unwrap_or_default();
        service.dns_search = raw_svc.dns_search.map(|d| d.into_vec()).unwrap_or_default();
        service.dns_opt = raw_svc.dns_opt.map(|d| d.into_vec()).unwrap_or_default();
        service.user = raw_svc.user;
        service.working_dir = raw_svc.working_dir;

        if let Some(raw_labels) = raw_svc.labels {
            match raw_labels {
                RawLabels::List(list) => {
                    for item in list {
                        if let Some((k, v)) = item.split_once('=') {
                            service.labels.insert(k.to_string(), v.to_string());
                        } else {
                            service.labels.insert(item, "".to_string());
                        }
                    }
                }
                RawLabels::Map(map) => service.labels = map,
            }
        }

        service.mem_limit = raw_svc.mem_limit;
        service.cpus = raw_svc.cpus.map(|c| match c {
            serde_yaml::Value::Number(n) => n.to_string(),
            serde_yaml::Value::String(s) => s,
            _ => "".to_string(),
        });
        service.shm_size = raw_svc.shm_size;
        service.stop_signal = raw_svc.stop_signal;
        service.gpus = raw_svc.gpus;
        service.stdin_open = raw_svc.stdin_open;
        service.tty = raw_svc.tty;
        service.replicas = raw_svc.replicas.unwrap_or(1);
        service.profiles = raw_svc.profiles;
        service.restart = raw_svc.restart;

        project.services.insert(svc_name, service);
    }

    for (net_key, raw_net) in raw_compose.networks {
        let name = raw_net
            .as_ref()
            .and_then(|n| n.name.clone())
            .unwrap_or_else(|| format!("{}_{}", project.name, net_key));
        let external = raw_net.as_ref().map(|n| n.external).unwrap_or(false);
        project.networks.insert(
            net_key.clone(),
            NetworkConfig {
                key: net_key,
                name,
                external,
            },
        );
    }

    for (vol_key, raw_vol) in raw_compose.volumes {
        let name = raw_vol
            .as_ref()
            .and_then(|v| v.name.clone())
            .unwrap_or_else(|| format!("{}_{}", project.name, vol_key));
        let external = raw_vol.as_ref().map(|v| v.external).unwrap_or(false);
        project.volumes.insert(
            vol_key.clone(),
            VolumeConfig {
                key: vol_key,
                name,
                external,
            },
        );
    }

    // Apply rcompose.yml extension if provided
    if let Some(rcompose_str) = rcompose_content {
        let interpolated_ext = interpolate_string(rcompose_str, &env_map)?;
        let rcompose_file: RcomposeFile = serde_yaml::from_str(&interpolated_ext)?;
        for (svc_name, ext) in rcompose_file.services {
            if let Some(svc) = project.services.get_mut(&svc_name) {
                ext.apply_to(svc);
            }
        }
    }

    Ok(project)
}

pub fn load_project(
    compose_path: &Path,
    rcompose_path: Option<&Path>,
    mut opts: LoadOptions,
) -> Result<Project, LoaderError> {
    if !compose_path.is_file() {
        return Err(LoaderError::FileNotFound(compose_path.display().to_string()));
    }

    let compose_content = fs::read_to_string(compose_path)?;
    let rcompose_content = match rcompose_path {
        Some(path) if path.is_file() => Some(fs::read_to_string(path)?),
        _ => {
            let parent = compose_path.parent().unwrap_or_else(|| Path::new("."));
            find_rcompose_extension_file(parent).and_then(|p| fs::read_to_string(p).ok())
        }
    };

    if opts.project_name.is_none() {
        if let Some(parent) = compose_path.parent() {
            if let Some(dirname) = parent.file_name().and_then(|n| n.to_str()) {
                opts.project_name = Some(dirname.to_lowercase().replace(' ', "-"));
            }
        }
    }

    let mut project = load_project_from_str(
        &compose_content,
        rcompose_content.as_deref(),
        opts,
    )?;

    if let Some(parent) = compose_path.parent() {
        project.directory = parent.to_path_buf();
    }

    Ok(project)
}
