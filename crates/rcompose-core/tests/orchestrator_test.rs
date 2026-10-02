use rcompose_core::orchestrator::*;
use rcompose_engine::mock::MockEngine;
use rcompose_spec::model::*;

#[tokio::test]
async fn test_orchestrator_up_and_down_lifecycle() {
    let mut project = Project::new("sample");

    let mut db = Service::default();
    db.name = "db".to_string();
    db.image = Some("postgres:alpine".to_string());

    let mut web = Service::default();
    web.name = "web".to_string();
    web.image = Some("nginx:alpine".to_string());
    web.depends_on = vec!["db".to_string()];

    project.services.insert("db".to_string(), db);
    project.services.insert("web".to_string(), web);

    let engine = MockEngine::new();
    let orchestrator = Orchestrator::new(project, engine.clone());

    // 1. Run up
    let up_opts = UpOptions::default();
    let events = orchestrator.up(up_opts).await.unwrap();
    assert_eq!(events.len(), 2);

    // Verify containers created in MockEngine
    let ps = orchestrator.ps().await.unwrap();
    assert_eq!(ps.len(), 2);

    // 2. Run up again (idempotent / up-to-date)
    let events_idempotent = orchestrator.up(UpOptions::default()).await.unwrap();
    for ev in events_idempotent {
        assert_eq!(ev.action, rcompose_core::drift::DesiredAction::UpToDate);
    }

    // 3. Run down
    let down_opts = DownOptions::default();
    orchestrator.down(down_opts).await.unwrap();

    let ps_after_down = orchestrator.ps().await.unwrap();
    assert_eq!(ps_after_down.len(), 0);
}
