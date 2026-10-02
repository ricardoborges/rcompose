use rcompose_engine::engine::*;
use rcompose_engine::mock::MockEngine;

#[tokio::test]
async fn test_mock_engine_lifecycle() {
    let engine = MockEngine::new();

    // Verify ping
    assert!(engine.ping().await.is_ok());

    // Create network & volume
    engine.create_network("my-net").await.unwrap();
    assert!(engine.list_networks().await.unwrap().contains(&"my-net".to_string()));

    engine.create_volume("my-vol").await.unwrap();
    assert!(engine.list_volumes().await.unwrap().contains(&"my-vol".to_string()));

    // Run container
    let opts = RunOptions {
        name: "test-web-1".to_string(),
        image: "nginx:alpine".to_string(),
        ..Default::default()
    };
    let id = engine.run_container(opts).await.unwrap();
    assert_eq!(id, "test-web-1");

    // Inspect container
    let details = engine.inspect_container("test-web-1").await.unwrap().unwrap();
    assert_eq!(details.name, "test-web-1");
    assert!(details.running);

    // Stop container
    engine.stop_container("test-web-1", 10).await.unwrap();
    let details_stopped = engine.inspect_container("test-web-1").await.unwrap().unwrap();
    assert!(!details_stopped.running);

    // Remove container
    engine.remove_container("test-web-1", true).await.unwrap();
    assert!(engine.inspect_container("test-web-1").await.unwrap().is_none());
}
