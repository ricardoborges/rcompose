//! Extensions schema and overlay logic for rcompose.yml (WSL-specific parameters).

use crate::model::Service;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RcomposeFile {
    pub version: Option<String>,
    #[serde(default)]
    pub services: HashMap<String, RcomposeServiceExtension>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RcomposeServiceExtension {
    #[serde(default)]
    pub wsl: Option<WslExtension>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WslExtension {
    pub session: Option<String>,
    pub gpus: Option<String>,
    pub memory_mb: Option<u64>,
    pub cpus: Option<u32>,
}

impl RcomposeServiceExtension {
    pub fn apply_to(&self, service: &mut Service) {
        if let Some(ref wsl) = self.wsl {
            if let Some(ref session) = wsl.session {
                service.wsl_session = Some(session.clone());
            }
            if let Some(ref gpus) = wsl.gpus {
                service.gpus = Some(gpus.clone());
            }
            if let Some(mem_mb) = wsl.memory_mb {
                service.mem_limit = Some(format!("{}m", mem_mb));
            }
            if let Some(cpus) = wsl.cpus {
                service.cpus = Some(cpus.to_string());
            }
        }
    }
}
