use rcompose_core::drift::*;
use rcompose_engine::engine::ContainerDetails;
use rcompose_spec::model::Service;

#[test]
fn test_compute_config_hash_stability() {
    let mut svc1 = Service::default();
    svc1.name = "web".to_string();
    svc1.image = Some("nginx:1.25".to_string());
    svc1.environment.insert("A".to_string(), Some("1".to_string()));
    svc1.environment.insert("B".to_string(), Some("2".to_string()));

    let mut svc2 = Service::default();
    svc2.name = "web".to_string();
    svc2.image = Some("nginx:1.25".to_string());
    // Insert in opposite order
    svc2.environment.insert("B".to_string(), Some("2".to_string()));
    svc2.environment.insert("A".to_string(), Some("1".to_string()));

    let hash1 = compute_config_hash(&svc1);
    let hash2 = compute_config_hash(&svc2);

    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 16);

    // Modify a property and verify hash changes
    svc2.image = Some("nginx:1.26".to_string());
    let hash3 = compute_config_hash(&svc2);
    assert_ne!(hash1, hash3);
}

#[test]
fn test_reconcile_service_state() {
    let mut svc = Service::default();
    svc.name = "web".to_string();
    svc.image = Some("nginx:alpine".to_string());
    let expected_hash = compute_config_hash(&svc);

    // 1. No existing container -> Create
    assert_eq!(
        reconcile_service_state(&svc, None, false),
        DesiredAction::Create
    );

    // 2. Existing running with same hash -> UpToDate
    let matching_running = ContainerDetails {
        id: "c1".to_string(),
        name: "test-web-1".to_string(),
        image: "nginx:alpine".to_string(),
        state: "running".to_string(),
        running: true,
        config_hash: Some(expected_hash.clone()),
        ..Default::default()
    };
    assert_eq!(
        reconcile_service_state(&svc, Some(&matching_running), false),
        DesiredAction::UpToDate
    );

    // 3. Existing stopped with same hash -> Start
    let matching_stopped = ContainerDetails {
        running: false,
        state: "stopped".to_string(),
        ..matching_running.clone()
    };
    assert_eq!(
        reconcile_service_state(&svc, Some(&matching_stopped), false),
        DesiredAction::Start
    );

    // 4. Hash mismatch -> Recreate
    let outdated = ContainerDetails {
        config_hash: Some("old_hash_1234567".to_string()),
        ..matching_running.clone()
    };
    assert_eq!(
        reconcile_service_state(&svc, Some(&outdated), false),
        DesiredAction::Recreate
    );

    // 5. Force recreate -> Recreate
    assert_eq!(
        reconcile_service_state(&svc, Some(&matching_running), true),
        DesiredAction::Recreate
    );
}

#[test]
fn test_hash_ignores_non_container_fields() {
    let mut svc = Service::default();
    svc.name = "web".to_string();
    svc.image = Some("nginx".to_string());
    let base = compute_config_hash(&svc);

    svc.depends_on = vec!["db".to_string()];
    svc.replicas = 3;
    svc.restart = Some("always".to_string());
    assert_eq!(compute_config_hash(&svc), base);

    svc.hostname = Some("web.local".to_string());
    assert_ne!(compute_config_hash(&svc), base);
}
