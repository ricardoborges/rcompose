//! Configuration drift detection and state reconciliation.

use rcompose_engine::engine::ContainerDetails;
use rcompose_spec::model::Service;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesiredAction {
    Create,
    Start,
    UpToDate,
    Recreate,
}

/// Computes a canonical SHA-256 hash (16-char hex) of the service configuration.
pub fn compute_config_hash(service: &Service) -> String {
    let mut sorted_env = BTreeMap::new();
    for (k, v) in &service.environment {
        sorted_env.insert(k.clone(), v.clone());
    }

    let mut sorted_labels = BTreeMap::new();
    for (k, v) in &service.labels {
        // Exclude internal compose labels from the hash calculation
        if !k.starts_with("com.docker.compose.") {
            sorted_labels.insert(k.clone(), v.clone());
        }
    }

    let canonical = serde_json::json!({
        "image": &service.image,
        "build": &service.build,
        "command": &service.command,
        "entrypoint": &service.entrypoint,
        "environment": sorted_env,
        "ports": &service.ports,
        "volumes": &service.volumes,
        "tmpfs": &service.tmpfs,
        "networks": &service.networks,
        "user": &service.user,
        "working_dir": &service.working_dir,
        "labels": sorted_labels,
        "mem_limit": &service.mem_limit,
        "cpus": &service.cpus,
        "shm_size": &service.shm_size,
        "ulimits": &service.ulimits,
        "stop_signal": &service.stop_signal,
        "gpus": &service.gpus,
        "wsl_session": &service.wsl_session,
    });

    let json_bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&json_bytes);
    let result = hasher.finalize();
    format!("{:x}", result)[..16].to_string()
}

/// Compares current service configuration against an existing container to determine lifecycle action.
pub fn reconcile_service_state(
    service: &Service,
    existing: Option<&ContainerDetails>,
    force_recreate: bool,
) -> DesiredAction {
    let existing = match existing {
        Some(e) => e,
        None => return DesiredAction::Create,
    };

    if force_recreate {
        return DesiredAction::Recreate;
    }

    let current_hash = compute_config_hash(service);

    if let Some(ref prev_hash) = existing.config_hash {
        if prev_hash == &current_hash {
            if existing.running {
                DesiredAction::UpToDate
            } else {
                DesiredAction::Start
            }
        } else {
            DesiredAction::Recreate
        }
    } else {
        // Container has no recorded config hash, recreate it to inject the hash
        DesiredAction::Recreate
    }
}
