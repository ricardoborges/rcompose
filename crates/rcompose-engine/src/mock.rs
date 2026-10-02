//! In-memory mock implementation of ContainerEngine for unit tests.

use crate::engine::*;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct MockEngine {
    containers: Arc<Mutex<HashMap<String, ContainerDetails>>>,
    networks: Arc<Mutex<HashSet<String>>>,
    volumes: Arc<Mutex<HashSet<String>>>,
    images: Arc<Mutex<HashSet<String>>>,
    pub run_history: Arc<Mutex<Vec<RunOptions>>>,
}

impl MockEngine {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ContainerEngine for MockEngine {
    async fn ping(&self) -> Result<(), EngineError> {
        Ok(())
    }

    async fn list_containers(&self, project: &str) -> Result<Vec<ContainerSummary>, EngineError> {
        let lock = self.containers.lock().unwrap();
        let mut list = Vec::new();
        for (name, details) in lock.iter() {
            if let Some(proj) = details.labels.get("com.docker.compose.project") {
                if proj == project {
                    list.push(ContainerSummary {
                        id: details.id.clone(),
                        name: name.clone(),
                        image: details.image.clone(),
                        service: details
                            .labels
                            .get("com.docker.compose.service")
                            .cloned()
                            .unwrap_or_default(),
                        status: if details.running {
                            "running".to_string()
                        } else {
                            "stopped".to_string()
                        },
                        ports: Vec::new(),
                        labels: details.labels.clone(),
                    });
                }
            }
        }
        Ok(list)
    }

    async fn inspect_container(&self, id_or_name: &str) -> Result<Option<ContainerDetails>, EngineError> {
        let lock = self.containers.lock().unwrap();
        Ok(lock.get(id_or_name).cloned())
    }

    async fn run_container(&self, opts: RunOptions) -> Result<String, EngineError> {
        let mut lock = self.containers.lock().unwrap();
        let name = opts.name.clone();
        let id = name.clone();
        let config_hash = opts.labels.get("com.docker.compose.config-hash").cloned();

        let details = ContainerDetails {
            id: id.clone(),
            name: name.clone(),
            image: opts.image.clone(),
            state: "running".to_string(),
            running: true,
            labels: opts.labels.clone(),
            config_hash,
        };

        self.run_history.lock().unwrap().push(opts);
        lock.insert(name, details);
        Ok(id)
    }

    async fn start_container(&self, id_or_name: &str) -> Result<(), EngineError> {
        let mut lock = self.containers.lock().unwrap();
        if let Some(c) = lock.get_mut(id_or_name) {
            c.running = true;
            c.state = "running".to_string();
            Ok(())
        } else {
            Err(EngineError::NotFound(id_or_name.to_string()))
        }
    }

    async fn stop_container(&self, id_or_name: &str, _timeout_secs: u32) -> Result<(), EngineError> {
        let mut lock = self.containers.lock().unwrap();
        if let Some(c) = lock.get_mut(id_or_name) {
            c.running = false;
            c.state = "stopped".to_string();
            Ok(())
        } else {
            Err(EngineError::NotFound(id_or_name.to_string()))
        }
    }

    async fn remove_container(&self, id_or_name: &str, _force: bool) -> Result<(), EngineError> {
        let mut lock = self.containers.lock().unwrap();
        lock.remove(id_or_name);
        Ok(())
    }

    async fn create_network(&self, name: &str) -> Result<(), EngineError> {
        let mut lock = self.networks.lock().unwrap();
        lock.insert(name.to_string());
        Ok(())
    }

    async fn list_networks(&self) -> Result<Vec<String>, EngineError> {
        let lock = self.networks.lock().unwrap();
        Ok(lock.iter().cloned().collect())
    }

    async fn create_volume(&self, name: &str) -> Result<(), EngineError> {
        let mut lock = self.volumes.lock().unwrap();
        lock.insert(name.to_string());
        Ok(())
    }

    async fn list_volumes(&self) -> Result<Vec<String>, EngineError> {
        let lock = self.volumes.lock().unwrap();
        Ok(lock.iter().cloned().collect())
    }

    async fn build_image(&self, opts: BuildOptions) -> Result<(), EngineError> {
        let mut lock = self.images.lock().unwrap();
        lock.insert(opts.tag);
        Ok(())
    }

    async fn image_exists(&self, image: &str) -> Result<bool, EngineError> {
        let lock = self.images.lock().unwrap();
        Ok(lock.contains(image))
    }
}
