//! In-memory mock implementation of ContainerEngine for unit tests.

use crate::engine::*;
use async_trait::async_trait;
use rcompose_spec::model::{LABEL_CONFIG_HASH, LABEL_INDEX, LABEL_PROJECT, LABEL_SERVICE};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct MockEngine {
    containers: Arc<Mutex<HashMap<String, ContainerDetails>>>,
    networks: Arc<Mutex<HashSet<String>>>,
    volumes: Arc<Mutex<HashSet<String>>>,
    images: Arc<Mutex<HashSet<String>>>,
    /// Exit code reported when a container started from this image stops on its own.
    exit_codes: Arc<Mutex<HashMap<String, i64>>>,
    pub create_history: Arc<Mutex<Vec<CreateOptions>>>,
    pub build_history: Arc<Mutex<Vec<BuildOptions>>>,
    /// Ordered log of engine calls, e.g. `start web-1`, `connect net web-1`.
    pub calls: Arc<Mutex<Vec<String>>>,
}

impl MockEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Containers of this image exit with `code` right after starting (one-shot jobs).
    pub fn set_exits_with(&self, image: &str, code: i64) {
        self.exit_codes.lock().unwrap().insert(image.to_string(), code);
    }

    pub fn add_image(&self, image: &str) {
        self.images.lock().unwrap().insert(Self::image_key(image));
    }

    pub fn add_network(&self, name: &str) {
        self.networks.lock().unwrap().insert(name.to_string());
    }

    fn log(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }

    fn image_key(image: &str) -> String {
        let (repo, tag) = normalize_image_ref(image);
        format!("{}:{}", repo, tag)
    }
}

#[async_trait]
impl ContainerEngine for MockEngine {
    async fn ping(&self) -> Result<(), EngineError> {
        Ok(())
    }

    async fn list_containers(&self, project: &str) -> Result<Vec<ContainerDetails>, EngineError> {
        let lock = self.containers.lock().unwrap();
        Ok(lock
            .values()
            .filter(|c| c.labels.get(LABEL_PROJECT).map(String::as_str) == Some(project))
            .cloned()
            .collect())
    }

    async fn inspect_container(&self, id_or_name: &str) -> Result<Option<ContainerDetails>, EngineError> {
        Ok(self.containers.lock().unwrap().get(id_or_name).cloned())
    }

    async fn create_container(&self, opts: CreateOptions) -> Result<String, EngineError> {
        self.log(format!("create {}", opts.name));
        let labels: HashMap<String, String> = opts.labels.clone().into_iter().collect();
        let details = ContainerDetails {
            id: opts.name.clone(),
            name: opts.name.clone(),
            image: opts.image.clone(),
            service: labels.get(LABEL_SERVICE).cloned().unwrap_or_default(),
            number: labels.get(LABEL_INDEX).and_then(|n| n.parse().ok()).unwrap_or(1),
            state: "created".to_string(),
            running: false,
            health: None,
            exit_code: None,
            ports: Vec::new(),
            config_hash: labels.get(LABEL_CONFIG_HASH).cloned(),
            labels,
        };
        self.images.lock().unwrap().insert(Self::image_key(&opts.image));
        self.containers.lock().unwrap().insert(opts.name.clone(), details);
        let name = opts.name.clone();
        self.create_history.lock().unwrap().push(opts);
        Ok(name)
    }

    async fn start_container(&self, id_or_name: &str) -> Result<(), EngineError> {
        self.log(format!("start {}", id_or_name));
        let exit_codes = self.exit_codes.lock().unwrap().clone();
        let history = self.create_history.lock().unwrap().clone();
        let mut lock = self.containers.lock().unwrap();
        let c = lock.get_mut(id_or_name).ok_or_else(|| EngineError::NotFound(id_or_name.to_string()))?;
        let has_healthcheck = history
            .iter()
            .rev()
            .find(|o| o.name == id_or_name)
            .is_some_and(|o| o.healthcheck.as_ref().is_some_and(|h| !h.disable));
        match exit_codes.get(&c.image) {
            Some(code) => {
                c.running = false;
                c.state = "exited".to_string();
                c.exit_code = Some(*code);
            }
            None => {
                c.running = true;
                c.state = "running".to_string();
                c.exit_code = Some(0);
                c.health = has_healthcheck.then(|| "healthy".to_string());
            }
        }
        Ok(())
    }

    async fn stop_container(&self, id_or_name: &str, _timeout_secs: Option<u32>) -> Result<(), EngineError> {
        self.log(format!("stop {}", id_or_name));
        let mut lock = self.containers.lock().unwrap();
        let c = lock.get_mut(id_or_name).ok_or_else(|| EngineError::NotFound(id_or_name.to_string()))?;
        c.running = false;
        c.state = "exited".to_string();
        Ok(())
    }

    async fn remove_container(&self, id_or_name: &str, _force: bool) -> Result<(), EngineError> {
        self.log(format!("remove {}", id_or_name));
        self.containers.lock().unwrap().remove(id_or_name);
        Ok(())
    }

    async fn connect_network(&self, network: &str, container: &str, aliases: &[String]) -> Result<(), EngineError> {
        self.log(format!("connect {} {} {}", network, container, aliases.join(",")));
        if !self.networks.lock().unwrap().contains(network) {
            return Err(EngineError::NotFound(network.to_string()));
        }
        Ok(())
    }

    async fn create_network(&self, name: &str, _labels: &BTreeMap<String, String>) -> Result<(), EngineError> {
        self.log(format!("network create {}", name));
        self.networks.lock().unwrap().insert(name.to_string());
        Ok(())
    }

    async fn remove_network(&self, name: &str) -> Result<(), EngineError> {
        self.log(format!("network remove {}", name));
        self.networks.lock().unwrap().remove(name);
        Ok(())
    }

    async fn list_networks(&self) -> Result<Vec<String>, EngineError> {
        Ok(self.networks.lock().unwrap().iter().cloned().collect())
    }

    async fn create_volume(&self, name: &str, _labels: &BTreeMap<String, String>) -> Result<(), EngineError> {
        self.log(format!("volume create {}", name));
        self.volumes.lock().unwrap().insert(name.to_string());
        Ok(())
    }

    async fn remove_volume(&self, name: &str) -> Result<(), EngineError> {
        self.log(format!("volume remove {}", name));
        self.volumes.lock().unwrap().remove(name);
        Ok(())
    }

    async fn list_volumes(&self) -> Result<Vec<String>, EngineError> {
        Ok(self.volumes.lock().unwrap().iter().cloned().collect())
    }

    async fn build_image(&self, opts: BuildOptions) -> Result<(), EngineError> {
        self.log(format!("build {}", opts.tag));
        self.images.lock().unwrap().insert(Self::image_key(&opts.tag));
        self.build_history.lock().unwrap().push(opts);
        Ok(())
    }

    async fn pull_image(&self, image: &str) -> Result<(), EngineError> {
        self.log(format!("pull {}", image));
        self.images.lock().unwrap().insert(Self::image_key(image));
        Ok(())
    }

    async fn image_exists(&self, image: &str) -> Result<bool, EngineError> {
        Ok(self.images.lock().unwrap().contains(&Self::image_key(image)))
    }
}
