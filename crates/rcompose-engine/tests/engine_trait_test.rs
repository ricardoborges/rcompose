use rcompose_engine::engine::*;
use rcompose_engine::mock::MockEngine;
use std::collections::BTreeMap;

#[tokio::test]
async fn test_mock_engine_lifecycle() {
    let engine = MockEngine::new();
    let no_labels = BTreeMap::new();

    assert!(engine.ping().await.is_ok());

    engine.create_network("my-net", &no_labels).await.unwrap();
    assert!(engine.list_networks().await.unwrap().contains(&"my-net".to_string()));
    engine.create_volume("my-vol", &no_labels).await.unwrap();
    assert!(engine.list_volumes().await.unwrap().contains(&"my-vol".to_string()));

    let opts = CreateOptions {
        name: "test-web-1".to_string(),
        image: "nginx:alpine".to_string(),
        ..Default::default()
    };
    let id = engine.create_container(opts).await.unwrap();
    assert_eq!(id, "test-web-1");
    assert!(!engine.inspect_container("test-web-1").await.unwrap().unwrap().running);

    engine.start_container("test-web-1").await.unwrap();
    assert!(engine.inspect_container("test-web-1").await.unwrap().unwrap().running);

    engine.stop_container("test-web-1", Some(10)).await.unwrap();
    assert!(!engine.inspect_container("test-web-1").await.unwrap().unwrap().running);

    engine.remove_container("test-web-1", true).await.unwrap();
    assert!(engine.inspect_container("test-web-1").await.unwrap().is_none());

    engine.remove_network("my-net").await.unwrap();
    engine.remove_volume("my-vol").await.unwrap();
    assert!(engine.list_networks().await.unwrap().is_empty());
    assert!(engine.list_volumes().await.unwrap().is_empty());
}

#[test]
fn test_normalize_image_ref() {
    assert_eq!(normalize_image_ref("nginx"), ("nginx".into(), "latest".into()));
    assert_eq!(normalize_image_ref("docker.io/library/redis:7"), ("redis".into(), "7".into()));
    assert_eq!(
        normalize_image_ref("ghcr.io/coollabsio/coolify:latest"),
        ("ghcr.io/coollabsio/coolify".into(), "latest".into())
    );
    assert_eq!(normalize_image_ref("localhost:5000/app"), ("localhost:5000/app".into(), "latest".into()));
}
