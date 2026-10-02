//! Compose file loading: YAML merge keys, variable interpolation and normalization into a [`Project`].

use crate::interpolation::{interpolate_value, load_env_file, parse_env_content};
use crate::model::*;
use serde_yaml::{Mapping, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LoaderError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Interpolation error: {0}")]
    Interpolation(#[from] crate::interpolation::InterpolationError),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Invalid Compose structure: {0}")]
    InvalidStructure(String),
}

#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// `-p` override; takes precedence over `COMPOSE_PROJECT_NAME`, `name:` and the directory name.
    pub project_name: Option<String>,
    /// Interpolation environment. When `None`, it is built from the `.env` file and the process env.
    pub env: Option<HashMap<String, String>>,
    /// Alternate `.env` file (defaults to `<working_dir>/.env`).
    pub env_file: Option<PathBuf>,
    /// Active profiles, in addition to those in `COMPOSE_PROFILES`.
    pub profiles: Vec<String>,
    /// Directory relative paths are resolved against (defaults to the compose file's directory).
    pub working_dir: Option<PathBuf>,
}

const COMPOSE_FILENAMES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// Service keys rcompose understands.
const SUPPORTED_SERVICE_KEYS: &[&str] = &[
    "image", "build", "command", "entrypoint", "container_name", "env_file", "environment",
    "ports", "expose", "volumes", "tmpfs", "networks", "depends_on", "healthcheck", "hostname",
    "domainname", "dns", "dns_search", "dns_opt", "user", "working_dir", "labels", "mem_limit",
    "cpus", "shm_size", "ulimits", "stop_signal", "stop_grace_period", "gpus", "stdin_open", "tty",
    "deploy", "scale", "profiles", "restart",
];

/// Valid Compose keys the wslc engine cannot honor.
const UNSUPPORTED_SERVICE_KEYS: &[&str] = &[
    "cap_add", "cap_drop", "privileged", "devices", "extra_hosts", "sysctls", "secrets", "configs",
    "init", "pid", "ipc", "read_only", "security_opt", "logging", "network_mode", "links",
    "external_links", "platform", "pull_policy", "volumes_from", "runtime", "userns_mode",
    "cgroup_parent", "mem_reservation", "memswap_limit", "mem_swappiness", "cpu_shares", "cpuset",
    "oom_score_adj", "blkio_config", "storage_opt", "isolation", "mac_address", "annotations",
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

pub fn load_project(compose_path: &Path, mut opts: LoadOptions) -> Result<Project, LoaderError> {
    if !compose_path.is_file() {
        return Err(LoaderError::FileNotFound(compose_path.display().to_string()));
    }
    let compose_path = std::path::absolute(compose_path)?;
    if opts.working_dir.is_none() {
        opts.working_dir = compose_path.parent().map(Path::to_path_buf);
    }
    let content = fs::read_to_string(&compose_path)?;
    load_project_from_str(&content, opts)
}

pub fn load_project_from_str(content: &str, opts: LoadOptions) -> Result<Project, LoaderError> {
    let working_dir = std::path::absolute(match opts.working_dir {
        Some(ref d) => d.clone(),
        None => std::env::current_dir()?,
    })?;

    let env = match opts.env {
        Some(ref e) => e.clone(),
        None => {
            let dotenv = match opts.env_file {
                Some(ref f) if !f.is_file() => {
                    return Err(LoaderError::FileNotFound(f.display().to_string()))
                }
                Some(ref f) => f.clone(),
                None => working_dir.join(".env"),
            };
            let mut env = load_env_file(&dotenv);
            env.extend(std::env::vars());
            env
        }
    };

    let mut root: Value = serde_yaml::from_str(content)?;
    root.apply_merge()?;
    interpolate_value(&mut root, &env)?;
    let top = match root {
        Value::Mapping(m) => m,
        _ => return Err(invalid("top level must be a mapping")),
    };

    let mut warnings = Vec::new();
    for (key, _) in top.iter() {
        let key = key.as_str().unwrap_or_default();
        match key {
            "version" | "name" | "services" | "networks" | "volumes" => {}
            k if k.starts_with("x-") => {}
            "include" => return Err(invalid("'include' is not supported yet")),
            "secrets" | "configs" => warnings.push(format!("top-level '{}' is not supported (ignored)", key)),
            other => warnings.push(format!("unknown top-level key '{}' (ignored)", other)),
        }
    }

    let raw_name = opts
        .project_name
        .clone()
        .or_else(|| env.get("COMPOSE_PROJECT_NAME").filter(|v| !v.is_empty()).cloned())
        .or_else(|| get(&top, "name").and_then(scalar))
        .or_else(|| working_dir.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_default();
    let mut project = Project::new(normalize_project_name(&raw_name)?);
    project.directory = working_dir.clone();
    project.warnings = warnings;

    for (key, cfg) in mapping_entries(get(&top, "networks"), "networks")? {
        let (name, external) = resource_name(&project.name, &key, cfg);
        project.networks.insert(key.clone(), NetworkConfig { key, name, external });
    }
    for (key, cfg) in mapping_entries(get(&top, "volumes"), "volumes")? {
        let (name, external) = resource_name(&project.name, &key, cfg);
        project.volumes.insert(key.clone(), VolumeConfig { key, name, external });
    }

    let mut active_profiles: BTreeSet<String> = opts.profiles.iter().cloned().collect();
    if let Some(p) = env.get("COMPOSE_PROFILES") {
        active_profiles.extend(p.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from));
    }

    let services = mapping_entries(get(&top, "services"), "services")?;
    if services.is_empty() {
        return Err(invalid("no services defined"));
    }
    for (svc_name, cfg) in services {
        let cfg = match cfg {
            Some(Value::Mapping(m)) => m,
            _ => return Err(invalid(format!("service '{}' must be a mapping", svc_name))),
        };
        let profiles = match get(cfg, "profiles") {
            Some(v) => string_list(v, &format!("services.{}.profiles", svc_name))?,
            None => Vec::new(),
        };
        let enabled = profiles.is_empty()
            || active_profiles.contains("*")
            || profiles.iter().any(|p| active_profiles.contains(p));
        if !enabled {
            project.disabled_services.insert(svc_name);
            continue;
        }
        let service = ServiceParser { project: &mut project, env: &env, working_dir: &working_dir }
            .parse(&svc_name, cfg)?;
        project.services.insert(svc_name, service);
    }

    validate_dependencies(&mut project)?;
    Ok(project)
}

/// Lowercases and strips characters Compose does not allow in project names.
pub fn normalize_project_name(raw: &str) -> Result<String, LoaderError> {
    let name: String = raw
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let name = name.trim_start_matches(['_', '-']).to_string();
    if name.is_empty() {
        return Err(invalid(format!("cannot derive a valid project name from '{}'", raw)));
    }
    Ok(name)
}

fn validate_dependencies(project: &mut Project) -> Result<(), LoaderError> {
    let enabled: BTreeSet<String> = project.services.keys().cloned().collect();
    for service in project.services.values_mut() {
        let mut kept = Vec::new();
        for dep in std::mem::take(&mut service.depends_on) {
            if enabled.contains(&dep) {
                kept.push(dep);
                continue;
            }
            if !service.optional_dependencies.contains(&dep) {
                let reason = if project.disabled_services.contains(&dep) {
                    "is disabled by its profiles"
                } else {
                    "is not defined"
                };
                return Err(invalid(format!(
                    "service '{}' depends on service '{}', which {}",
                    service.name, dep, reason
                )));
            }
            project
                .warnings
                .push(format!("{}: optional dependency '{}' is not available (ignored)", service.name, dep));
        }
        service.depends_on = kept;
    }
    Ok(())
}

struct ServiceParser<'a> {
    project: &'a mut Project,
    env: &'a HashMap<String, String>,
    working_dir: &'a Path,
}

impl ServiceParser<'_> {
    fn parse(mut self, name: &str, cfg: &Mapping) -> Result<Service, LoaderError> {
        let field = |key: &str| format!("services.{}.{}", name, key);
        let mut svc = Service { name: name.to_string(), ..Default::default() };

        for (key, _) in cfg.iter() {
            let key = key.as_str().unwrap_or_default();
            if key.starts_with("x-") || SUPPORTED_SERVICE_KEYS.contains(&key) {
                continue;
            }
            let why = if UNSUPPORTED_SERVICE_KEYS.contains(&key) {
                "is not supported by the wslc engine"
            } else {
                "is not a known Compose key"
            };
            self.warn(name, format!("'{}' {} (ignored)", key, why));
        }

        svc.image = get(cfg, "image").and_then(scalar);
        if let Some(build) = get(cfg, "build") {
            svc.build = Some(self.parse_build(build, &field("build"))?);
        }
        if svc.image.is_none() && svc.build.is_none() {
            return Err(invalid(format!("service '{}' has neither an image nor a build context", name)));
        }

        svc.command = get(cfg, "command").map(|v| command_list(v, &field("command"))).transpose()?;
        svc.entrypoint = get(cfg, "entrypoint").map(|v| command_list(v, &field("entrypoint"))).transpose()?;
        svc.container_name = get(cfg, "container_name").and_then(scalar);

        // env_file contents first, `environment` overrides them
        if let Some(env_files) = get(cfg, "env_file") {
            for (path, required) in self.parse_env_files(env_files, &field("env_file"))? {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        for (k, v) in parse_env_content(&content) {
                            svc.environment.insert(k, Some(v));
                        }
                    }
                    Err(_) if !required => {}
                    Err(_) => return Err(LoaderError::FileNotFound(path.display().to_string())),
                }
            }
        }
        if let Some(env) = get(cfg, "environment") {
            for (k, v) in kv_map(env, &field("environment"))? {
                // `- VAR` without a value is taken from the interpolation environment
                let v = v.or_else(|| self.env.get(&k).cloned());
                svc.environment.insert(k, v);
            }
        }

        for spec in sequence(get(cfg, "ports"), &field("ports"))? {
            svc.ports.extend(parse_port(spec, &field("ports"))?);
        }
        for spec in sequence(get(cfg, "volumes"), &field("volumes"))? {
            if let Some(mount) = self.parse_volume(name, spec, &field("volumes"))? {
                svc.volumes.push(mount);
            }
        }
        if let Some(t) = get(cfg, "tmpfs") {
            svc.tmpfs = string_list(t, &field("tmpfs"))?;
        }

        self.parse_networks(&mut svc, get(cfg, "networks"), &field("networks"))?;
        self.parse_depends_on(&mut svc, get(cfg, "depends_on"), &field("depends_on"))?;
        if let Some(hc) = get(cfg, "healthcheck") {
            svc.healthcheck = Some(parse_healthcheck(hc, &field("healthcheck"))?);
        }

        svc.hostname = get(cfg, "hostname").and_then(scalar);
        svc.domainname = get(cfg, "domainname").and_then(scalar);
        svc.dns = get(cfg, "dns").map(|v| string_list(v, &field("dns"))).transpose()?.unwrap_or_default();
        svc.dns_search = get(cfg, "dns_search")
            .map(|v| string_list(v, &field("dns_search")))
            .transpose()?
            .unwrap_or_default();
        svc.dns_opt = get(cfg, "dns_opt").map(|v| string_list(v, &field("dns_opt"))).transpose()?.unwrap_or_default();
        svc.user = get(cfg, "user").and_then(scalar);
        svc.working_dir = get(cfg, "working_dir").and_then(scalar);
        if let Some(labels) = get(cfg, "labels") {
            svc.labels = kv_map(labels, &field("labels"))?
                .into_iter()
                .map(|(k, v)| (k, v.unwrap_or_default()))
                .collect();
        }

        svc.mem_limit = get(cfg, "mem_limit").and_then(scalar);
        svc.cpus = get(cfg, "cpus").and_then(scalar);
        svc.shm_size = get(cfg, "shm_size").and_then(scalar);
        svc.gpus = get(cfg, "gpus").and_then(scalar);
        if let Some(Value::Sequence(_)) = get(cfg, "gpus") {
            svc.gpus = Some("all".to_string());
        }
        if let Some(scale) = get(cfg, "scale").and_then(scalar) {
            svc.replicas = scale.parse().map_err(|_| invalid(format!("{}: invalid number", field("scale"))))?;
        }
        if let Some(deploy) = get(cfg, "deploy") {
            self.parse_deploy(&mut svc, deploy, &field("deploy"))?;
        }

        if let Some(Value::Mapping(ulimits)) = get(cfg, "ulimits") {
            for (key, limit) in ulimits {
                let key = key.as_str().unwrap_or_default();
                let spec = match limit {
                    Value::Mapping(m) => format!(
                        "{}={}:{}",
                        key,
                        get(m, "soft").and_then(scalar).unwrap_or_else(|| "-1".into()),
                        get(m, "hard").and_then(scalar).unwrap_or_else(|| "-1".into())
                    ),
                    other => format!("{}={}", key, scalar(other).unwrap_or_default()),
                };
                svc.ulimits.push(spec);
            }
        }

        svc.stop_signal = get(cfg, "stop_signal").and_then(scalar);
        if let Some(period) = get(cfg, "stop_grace_period").and_then(scalar) {
            svc.stop_grace_period = Some(parse_duration_secs(&period).ok_or_else(|| {
                invalid(format!("{}: invalid duration '{}'", field("stop_grace_period"), period))
            })?);
        }
        svc.stdin_open = get(cfg, "stdin_open").and_then(as_bool).unwrap_or(false);
        svc.tty = get(cfg, "tty").and_then(as_bool).unwrap_or(false);
        svc.profiles = get(cfg, "profiles").map(|v| string_list(v, &field("profiles"))).transpose()?.unwrap_or_default();
        svc.restart = get(cfg, "restart").and_then(scalar);
        if let Some(policy) = svc.restart.as_deref().filter(|p| *p != "no") {
            self.warn(name, format!("restart policy '{}' is not supported by the wslc engine (ignored)", policy));
        }

        Ok(svc)
    }

    fn warn(&mut self, service: &str, msg: String) {
        self.project.warnings.push(format!("{}: {}", service, msg));
    }

    fn parse_build(&self, value: &Value, what: &str) -> Result<BuildConfig, LoaderError> {
        let (context, cfg) = match value {
            Value::Mapping(m) => (get(m, "context").and_then(scalar).unwrap_or_else(|| ".".into()), Some(m)),
            other => (scalar(other).ok_or_else(|| invalid(format!("{}: expected a string or mapping", what)))?, None),
        };
        let context = resolve_path(&context, self.working_dir);
        let mut build = BuildConfig {
            context: context.clone(),
            dockerfile: None,
            args: BTreeMap::new(),
            target: None,
            pull: false,
            no_cache: false,
        };
        if let Some(m) = cfg {
            // wslc resolves -f against its own cwd: make the dockerfile absolute, relative to the context
            build.dockerfile = get(m, "dockerfile")
                .and_then(scalar)
                .map(|df| resolve_path(&df, Path::new(&context)));
            if let Some(args) = get(m, "args") {
                build.args = kv_map(args, &format!("{}.args", what))?
                    .into_iter()
                    .filter_map(|(k, v)| v.or_else(|| self.env.get(&k).cloned()).map(|v| (k, v)))
                    .collect();
            }
            build.target = get(m, "target").and_then(scalar);
            build.pull = get(m, "pull").and_then(as_bool).unwrap_or(false);
            build.no_cache = get(m, "no_cache").and_then(as_bool).unwrap_or(false);
        }
        Ok(build)
    }

    fn parse_env_files(&self, value: &Value, what: &str) -> Result<Vec<(PathBuf, bool)>, LoaderError> {
        let items = match value {
            Value::Sequence(seq) => seq.iter().collect(),
            other => vec![other],
        };
        let mut files = Vec::new();
        for item in items {
            let (path, required) = match item {
                Value::Mapping(m) => (
                    get(m, "path").and_then(scalar).ok_or_else(|| invalid(format!("{}: entry without 'path'", what)))?,
                    get(m, "required").and_then(as_bool).unwrap_or(true),
                ),
                other => (scalar(other).ok_or_else(|| invalid(format!("{}: invalid entry", what)))?, true),
            };
            files.push((PathBuf::from(resolve_path(&path, self.working_dir)), required));
        }
        Ok(files)
    }

    fn parse_volume(&mut self, service: &str, spec: &Value, what: &str) -> Result<Option<VolumeMount>, LoaderError> {
        let mut mount = match spec {
            Value::Mapping(m) => {
                let target = get(m, "target")
                    .and_then(scalar)
                    .ok_or_else(|| invalid(format!("{}: entry without 'target'", what)))?;
                let mount_type = match get(m, "type").and_then(scalar).as_deref() {
                    Some("bind") => VolumeType::Bind,
                    Some("tmpfs") => VolumeType::Tmpfs,
                    Some("volume") | None => VolumeType::Volume,
                    Some(other) => return Err(invalid(format!("{}: unsupported volume type '{}'", what, other))),
                };
                VolumeMount {
                    mount_type,
                    source: get(m, "source").and_then(scalar),
                    target,
                    read_only: get(m, "read_only").and_then(as_bool).unwrap_or(false),
                }
            }
            other => {
                let s = scalar(other).ok_or_else(|| invalid(format!("{}: invalid entry", what)))?;
                VolumeMount::parse(&s).map_err(|e| invalid(format!("{}: {}", what, e)))?
            }
        };

        match mount.mount_type {
            VolumeType::Tmpfs => {}
            VolumeType::Volume => {
                if let Some(ref source) = mount.source {
                    let vol = self.project.volumes.get(source).ok_or_else(|| {
                        invalid(format!("service '{}' refers to undefined volume '{}'", service, source))
                    })?;
                    mount.source = Some(vol.name.clone());
                }
            }
            VolumeType::Bind => {
                let source = mount.source.clone().unwrap_or_default();
                match resolve_bind_source(&source, self.working_dir) {
                    Some(resolved) => mount.source = Some(resolved),
                    None => {
                        self.warn(
                            service,
                            format!(
                                "bind mount '{}' is a Linux host path with no equivalent on a Windows host (mount skipped)",
                                source
                            ),
                        );
                        return Ok(None);
                    }
                }
            }
        }
        Ok(Some(mount))
    }

    fn parse_networks(&mut self, svc: &mut Service, value: Option<&Value>, what: &str) -> Result<(), LoaderError> {
        let entries: Vec<(String, Vec<String>)> = match value {
            None => vec![("default".to_string(), Vec::new())],
            Some(Value::Sequence(_)) => string_list(value.unwrap(), what)?.into_iter().map(|k| (k, Vec::new())).collect(),
            Some(Value::Mapping(m)) => {
                let mut entries = Vec::new();
                for (key, cfg) in m {
                    let key = scalar(key).ok_or_else(|| invalid(format!("{}: invalid network key", what)))?;
                    let aliases = match cfg {
                        Value::Mapping(c) => get(c, "aliases")
                            .map(|a| string_list(a, &format!("{}.{}.aliases", what, key)))
                            .transpose()?
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    entries.push((key, aliases));
                }
                entries
            }
            Some(_) => return Err(invalid(format!("{}: expected a list or mapping", what))),
        };

        for (key, aliases) in entries {
            if !self.project.networks.contains_key(&key) {
                if key != "default" {
                    return Err(invalid(format!("service '{}' refers to undefined network '{}'", svc.name, key)));
                }
                let name = self.project.default_network_name();
                self.project
                    .networks
                    .insert(key.clone(), NetworkConfig { key: key.clone(), name, external: false });
            }
            let net_name = self.project.networks[&key].name.clone();
            if !aliases.is_empty() {
                svc.network_aliases.insert(net_name.clone(), aliases);
            }
            svc.networks.push(net_name);
        }
        Ok(())
    }

    fn parse_depends_on(&mut self, svc: &mut Service, value: Option<&Value>, what: &str) -> Result<(), LoaderError> {
        match value {
            None => {}
            Some(Value::Sequence(_)) => svc.depends_on = string_list(value.unwrap(), what)?,
            Some(Value::Mapping(m)) => {
                for (dep, cfg) in m {
                    let dep = scalar(dep).ok_or_else(|| invalid(format!("{}: invalid service name", what)))?;
                    let cfg = match cfg {
                        Value::Mapping(c) => Some(c),
                        _ => None,
                    };
                    let condition = match cfg.and_then(|c| get(c, "condition")).and_then(scalar).as_deref() {
                        None | Some("service_started") => DependencyCondition::ServiceStarted,
                        Some("service_healthy") => DependencyCondition::ServiceHealthy,
                        Some("service_completed_successfully") => DependencyCondition::ServiceCompletedSuccessfully,
                        Some(other) => return Err(invalid(format!("{}.{}: unknown condition '{}'", what, dep, other))),
                    };
                    if cfg.and_then(|c| get(c, "required")).and_then(as_bool) == Some(false) {
                        svc.optional_dependencies.insert(dep.clone());
                    }
                    if condition != DependencyCondition::ServiceStarted {
                        svc.dependency_conditions.insert(dep.clone(), condition);
                    }
                    svc.depends_on.push(dep);
                }
            }
            Some(_) => return Err(invalid(format!("{}: expected a list or mapping", what))),
        }
        Ok(())
    }

    fn parse_deploy(&mut self, svc: &mut Service, value: &Value, what: &str) -> Result<(), LoaderError> {
        let Value::Mapping(deploy) = value else {
            return Err(invalid(format!("{}: expected a mapping", what)));
        };
        if let Some(replicas) = get(deploy, "replicas").and_then(scalar) {
            svc.replicas = replicas.parse().map_err(|_| invalid(format!("{}.replicas: invalid number", what)))?;
        }
        let resources = get(deploy, "resources");
        let section = |name: &str| match resources {
            Some(Value::Mapping(r)) => match get(r, name) {
                Some(Value::Mapping(s)) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        };
        if let Some(limits) = section("limits") {
            if let Some(mem) = get(&limits, "memory").and_then(scalar) {
                svc.mem_limit = Some(mem);
            }
            if let Some(cpus) = get(&limits, "cpus").and_then(scalar) {
                svc.cpus = Some(cpus);
            }
        }
        if let Some(reservations) = section("reservations") {
            for device in sequence(get(&reservations, "devices"), &format!("{}.resources.reservations.devices", what))? {
                let Value::Mapping(device) = device else { continue };
                let caps = get(device, "capabilities")
                    .map(|c| string_list(c, "capabilities"))
                    .transpose()?
                    .unwrap_or_default();
                if caps.iter().any(|c| c == "gpu") {
                    svc.gpus = Some(get(device, "count").and_then(scalar).unwrap_or_else(|| "all".into()));
                }
            }
        }
        Ok(())
    }
}

fn parse_port(spec: &Value, what: &str) -> Result<Vec<PortMapping>, LoaderError> {
    let text = match spec {
        Value::Mapping(m) => {
            let target = get(m, "target")
                .and_then(scalar)
                .ok_or_else(|| invalid(format!("{}: entry without 'target'", what)))?;
            let mut text = match (get(m, "host_ip").and_then(scalar), get(m, "published").and_then(scalar)) {
                (Some(ip), Some(p)) => format!("{}:{}:{}", ip, p, target),
                (Some(ip), None) => format!("{}::{}", ip, target),
                (None, Some(p)) => format!("{}:{}", p, target),
                (None, None) => target,
            };
            if let Some(proto) = get(m, "protocol").and_then(scalar) {
                text = format!("{}/{}", text, proto);
            }
            text
        }
        other => scalar(other).ok_or_else(|| invalid(format!("{}: invalid entry", what)))?,
    };
    PortMapping::parse(&text).map_err(|e| invalid(format!("{}: {}", what, e)))
}

fn parse_healthcheck(value: &Value, what: &str) -> Result<Healthcheck, LoaderError> {
    let Value::Mapping(m) = value else {
        return Err(invalid(format!("{}: expected a mapping", what)));
    };
    let test = match get(m, "test") {
        None => Vec::new(),
        Some(Value::Sequence(_)) => string_list(get(m, "test").unwrap(), &format!("{}.test", what))?,
        Some(other) => vec![
            "CMD-SHELL".to_string(),
            scalar(other).ok_or_else(|| invalid(format!("{}.test: invalid value", what)))?,
        ],
    };
    let disable = get(m, "disable").and_then(as_bool).unwrap_or(false) || test.first().map(String::as_str) == Some("NONE");
    Ok(Healthcheck {
        test,
        interval: get(m, "interval").and_then(scalar),
        timeout: get(m, "timeout").and_then(scalar),
        start_period: get(m, "start_period").and_then(scalar),
        retries: get(m, "retries")
            .and_then(scalar)
            .map(|r| r.parse().map_err(|_| invalid(format!("{}.retries: invalid number", what))))
            .transpose()?,
        disable,
    })
}

/// Parses a Compose duration (`1m30s`, `10s`, `500ms`, `2h`) into whole seconds.
pub fn parse_duration_secs(s: &str) -> Option<u32> {
    let mut total_ms: u64 = 0;
    let mut rest = s.trim();
    if rest.is_empty() {
        return None;
    }
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(rest.len());
        let number: f64 = rest[..digits].parse().ok()?;
        rest = &rest[digits..];
        let unit_len = rest.find(|c: char| c.is_ascii_digit()).unwrap_or(rest.len());
        let factor = match &rest[..unit_len] {
            "h" => 3_600_000.0,
            "m" => 60_000.0,
            "s" | "" => 1_000.0,
            "ms" => 1.0,
            "us" | "µs" => 0.001,
            _ => return None,
        };
        total_ms += (number * factor) as u64;
        rest = &rest[unit_len..];
    }
    Some(total_ms.div_ceil(1000) as u32)
}

/// Resolves a bind source to an absolute host path, or `None` when it has no meaning on this host.
fn resolve_bind_source(source: &str, working_dir: &Path) -> Option<String> {
    if is_windows_drive_path(source) || source.starts_with(r"\\") {
        return Some(source.to_string());
    }
    if cfg!(windows) && source.starts_with('/') {
        // /mnt/<drive>/... from a WSL-style path maps back to the Windows drive
        let mut chars = source.strip_prefix("/mnt/")?.chars();
        let drive = chars.next().filter(char::is_ascii_alphabetic)?;
        let tail = chars.as_str();
        if !(tail.is_empty() || tail.starts_with('/')) {
            return None;
        }
        return Some(format!(
            "{}:\\{}",
            drive.to_ascii_uppercase(),
            tail.trim_start_matches('/').replace('/', "\\")
        ));
    }
    Some(resolve_path(source, working_dir))
}

/// Makes `path` absolute against `base` (expanding `~`) and normalizes `.`/`..` lexically.
fn resolve_path(path: &str, base: &Path) -> String {
    let expanded = match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
            match home {
                Some(h) => PathBuf::from(h).join(rest.trim_start_matches(['/', '\\'])),
                None => PathBuf::from(path),
            }
        }
        _ => PathBuf::from(path),
    };
    let joined = if expanded.is_absolute() { expanded } else { base.join(expanded) };

    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized.to_string_lossy().to_string()
}

fn resource_name(project: &str, key: &str, cfg: Option<&Value>) -> (String, bool) {
    let cfg = match cfg {
        Some(Value::Mapping(m)) => Some(m),
        _ => None,
    };
    let external_value = cfg.and_then(|c| get(c, "external"));
    let external = match external_value {
        Some(Value::Mapping(_)) => true, // legacy `external: {name: ...}`
        Some(v) => as_bool(v).unwrap_or(false),
        None => false,
    };
    let legacy_name = match external_value {
        Some(Value::Mapping(m)) => get(m, "name").and_then(scalar),
        _ => None,
    };
    let name = cfg
        .and_then(|c| get(c, "name"))
        .and_then(scalar)
        .or(legacy_name)
        .unwrap_or_else(|| if external { key.to_string() } else { format!("{}_{}", project, key) });
    (name, external)
}

// --- YAML helpers -----------------------------------------------------------

fn invalid(msg: impl Into<String>) -> LoaderError {
    LoaderError::InvalidStructure(msg.into())
}

/// Looks up `key`, treating an explicit `null` as absent.
fn get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.get(key).filter(|v| !v.is_null())
}

/// Strings, numbers and booleans as a string.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.to_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn mapping_entries<'a>(value: Option<&'a Value>, what: &str) -> Result<Vec<(String, Option<&'a Value>)>, LoaderError> {
    match value {
        None => Ok(Vec::new()),
        Some(Value::Mapping(m)) => m
            .iter()
            .map(|(k, v)| {
                let key = scalar(k).ok_or_else(|| invalid(format!("{}: invalid key", what)))?;
                Ok((key, Some(v).filter(|v| !v.is_null())))
            })
            .collect(),
        Some(_) => Err(invalid(format!("'{}' must be a mapping", what))),
    }
}

fn sequence<'a>(value: Option<&'a Value>, what: &str) -> Result<&'a [Value], LoaderError> {
    match value {
        None => Ok(&[]),
        Some(Value::Sequence(seq)) => Ok(seq),
        Some(_) => Err(invalid(format!("{}: expected a list", what))),
    }
}

fn string_list(value: &Value, what: &str) -> Result<Vec<String>, LoaderError> {
    match value {
        Value::Sequence(seq) => seq
            .iter()
            .map(|v| scalar(v).ok_or_else(|| invalid(format!("{}: expected a list of strings", what))))
            .collect(),
        other => Ok(vec![scalar(other).ok_or_else(|| invalid(format!("{}: expected a string or list", what)))?]),
    }
}

/// A command as a string (split shell-style) or as a list.
fn command_list(value: &Value, what: &str) -> Result<Vec<String>, LoaderError> {
    match value {
        Value::Sequence(_) => string_list(value, what),
        other => {
            let s = scalar(other).ok_or_else(|| invalid(format!("{}: expected a string or list", what)))?;
            shell_words::split(&s).map_err(|e| invalid(format!("{}: {}", what, e)))
        }
    }
}

/// `KEY=VALUE` lists or mappings; a key without a value maps to `None`.
fn kv_map(value: &Value, what: &str) -> Result<BTreeMap<String, Option<String>>, LoaderError> {
    let mut out = BTreeMap::new();
    match value {
        Value::Mapping(m) => {
            for (k, v) in m {
                let key = scalar(k).ok_or_else(|| invalid(format!("{}: invalid key", what)))?;
                out.insert(key, if v.is_null() { None } else { scalar(v) });
            }
        }
        Value::Sequence(seq) => {
            for item in seq {
                let item = scalar(item).ok_or_else(|| invalid(format!("{}: expected KEY=VALUE strings", what)))?;
                match item.split_once('=') {
                    Some((k, v)) => out.insert(k.to_string(), Some(v.to_string())),
                    None => out.insert(item, None),
                };
            }
        }
        _ => return Err(invalid(format!("{}: expected a list or mapping", what))),
    }
    Ok(out)
}
