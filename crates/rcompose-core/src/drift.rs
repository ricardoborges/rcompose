//! Configuration drift detection and state reconciliation.

use rcompose_engine::engine::ContainerDetails;
use rcompose_spec::model::Service;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesiredAction {
    Create,
    Start,
    UpToDate,
    Recreate,
}

/// Service fields that do not change the container itself and so must not trigger a recreate.
const NON_CONTAINER_FIELDS: &[&str] = &[
    "name",
    "depends_on",
    "dependency_conditions",
    "optional_dependencies",
    "profiles",
    "replicas",
    "restart",
];

/// Computes a canonical SHA-256 hash (16-char hex) of the service configuration.
pub fn compute_config_hash(service: &Service) -> String {
    let mut canonical = serde_json::to_value(service).unwrap_or_default();
    if let Some(map) = canonical.as_object_mut() {
        for field in NON_CONTAINER_FIELDS {
            map.remove(*field);
        }
        if let Some(serde_json::Value::Object(labels)) = map.get_mut("labels") {
            labels.retain(|k, _| !k.starts_with("com.docker.compose."));
        }
    }

    // serde_json maps are ordered, so the serialization is stable
    let json_bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    let digest = Sha256::digest(&json_bytes);
    format!("{:x}", digest)[..16].to_string()
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

    match existing.config_hash {
        Some(ref prev_hash) if *prev_hash == compute_config_hash(service) => {
            if existing.running {
                DesiredAction::UpToDate
            } else {
                DesiredAction::Start
            }
        }
        // Changed configuration, or no recorded hash to compare against
        _ => DesiredAction::Recreate,
    }
}
