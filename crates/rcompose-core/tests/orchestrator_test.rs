use rcompose_core::drift::DesiredAction;
use rcompose_core::orchestrator::*;
use rcompose_engine::engine::ContainerEngine;
use rcompose_engine::mock::MockEngine;
use rcompose_spec::loader::{load_project_from_str, LoadOptions};
use rcompose_spec::model::*;
use std::collections::HashMap;

fn load(yaml: &str) -> Project {
    load_project_from_str(
        yaml,
        LoadOptions {
            project_name: Some("pk".to_string()),
            env: Some(HashMap::new()),
            ..Default::default()
        },
    )
    .unwrap()
}

fn calls(engine: &MockEngine) -> Vec<String> {
    engine.calls.lock().unwrap().clone()
}

fn index_of(calls: &[String], call: &str) -> usize {
    calls.iter().position(|c| c == call).unwrap_or_else(|| panic!("'{}' not in {:#?}", call, calls))
}

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

    let events = orchestrator.up(UpOptions::default()).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(orchestrator.ps().await.unwrap().len(), 2);

    // Second run is idempotent
    for ev in orchestrator.up(UpOptions::default()).await.unwrap() {
        assert_eq!(ev.action, DesiredAction::UpToDate);
    }

    orchestrator.down(DownOptions::default()).await.unwrap();
    assert_eq!(orchestrator.ps().await.unwrap().len(), 0);
}

const PAINKILLER_LIKE: &str = r#"
services:
  api:
    build: .
    image: pk-api:latest
    depends_on:
      gitea:
        condition: service_started
  builder:
    build: .
    image: pk-worker:latest
    profiles: ["build"]
  gitea:
    image: gitea/gitea:1.22
    networks:
      default:
      coolify:
        aliases: [gitea]
  coolify:
    image: coolify
    depends_on:
      db:
        condition: service_healthy
      ssh-init:
        condition: service_completed_successfully
    networks:
      default:
      coolify-ssh:
  ssh-init:
    image: alpine:3.20
    volumes:
      - ssh:/ssh
  db:
    image: postgres:16-alpine
    volumes:
      - db-data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready"]
networks:
  coolify-ssh:
  coolify:
    name: coolify
    external: true
volumes:
  ssh:
  db-data:
"#;

#[tokio::test]
async fn test_up_painkiller_like_project() {
    let engine = MockEngine::new();
    engine.add_network("coolify");
    engine.set_exits_with("alpine:3.20", 0);
    let orchestrator = Orchestrator::new(load(PAINKILLER_LIKE), engine.clone());

    let events = orchestrator.up(UpOptions::default()).await.unwrap();
    let mut started: Vec<&str> = events.iter().map(|e| e.service.as_str()).collect();
    started.sort();
    assert_eq!(started, vec!["api", "coolify", "db", "gitea", "ssh-init"]);

    let calls = calls(&engine);
    // only the non-profile service with a build section is built; the profile one is skipped
    assert!(calls.contains(&"build pk-api:latest".to_string()));
    assert!(!calls.contains(&"build pk-worker:latest".to_string()));
    // project networks/volumes are created, the external one is reused
    assert!(calls.contains(&"network create pk_default".to_string()));
    assert!(calls.contains(&"network create pk_coolify-ssh".to_string()));
    assert!(!calls.contains(&"network create coolify".to_string()));
    assert!(calls.contains(&"volume create pk_db-data".to_string()));
    // secondary networks are attached between create and start, with the service name as alias
    let connect = index_of(&calls, "connect coolify pk-gitea-1 gitea");
    assert!(index_of(&calls, "create pk-gitea-1") < connect && connect < index_of(&calls, "start pk-gitea-1"));
    // coolify starts only after its dependencies are healthy / completed
    assert!(index_of(&calls, "start pk-db-1") < index_of(&calls, "create pk-coolify-1"));
    assert!(index_of(&calls, "start pk-ssh-init-1") < index_of(&calls, "create pk-coolify-1"));

    // the primary network carries the service name as alias
    let history = engine.create_history.lock().unwrap().clone();
    let api = history.iter().find(|o| o.name == "pk-api-1").unwrap();
    assert_eq!(api.network.as_deref(), Some("pk_default"));
    assert_eq!(api.network_aliases, vec!["api"]);

    orchestrator.down(DownOptions { remove_volumes: true, ..Default::default() }).await.unwrap();
    let calls = self::calls(&engine);
    assert!(calls.contains(&"network remove pk_default".to_string()));
    assert!(!calls.contains(&"network remove coolify".to_string()));
    assert!(calls.contains(&"volume remove pk_db-data".to_string()));
    assert!(orchestrator.ps().await.unwrap().is_empty());
}

#[tokio::test]
async fn test_missing_external_network_fails() {
    let engine = MockEngine::new();
    let orchestrator = Orchestrator::new(load(PAINKILLER_LIKE), engine);
    let err = orchestrator.up(UpOptions::default()).await.unwrap_err();
    assert!(err.to_string().contains("network coolify declared as external"));
}

#[tokio::test]
async fn test_failed_one_shot_blocks_dependents() {
    let engine = MockEngine::new();
    engine.add_network("coolify");
    engine.set_exits_with("alpine:3.20", 3);
    let orchestrator = Orchestrator::new(load(PAINKILLER_LIKE), engine.clone());

    let err = orchestrator.up(UpOptions::default()).await.unwrap_err();
    assert!(err.to_string().contains("exited with code 3"), "{}", err);
    assert!(!calls(&engine).contains(&"create pk-coolify-1".to_string()));
}

#[tokio::test]
async fn test_config_change_recreates_and_rebuild_forces_recreate() {
    let yaml = "services:\n  web:\n    image: nginx\n    environment:\n      A: \"1\"\n";
    let engine = MockEngine::new();
    Orchestrator::new(load(yaml), engine.clone()).up(UpOptions::default()).await.unwrap();

    let changed = yaml.replace("\"1\"", "\"2\"");
    let events = Orchestrator::new(load(&changed), engine.clone()).up(UpOptions::default()).await.unwrap();
    assert_eq!(events[0].action, DesiredAction::Recreate);

    let built = "services:\n  app:\n    build: .\n";
    Orchestrator::new(load(built), engine.clone()).up(UpOptions::default()).await.unwrap();
    let events = Orchestrator::new(load(built), engine.clone())
        .up(UpOptions { build: true, ..Default::default() })
        .await
        .unwrap();
    assert_eq!(events[0].action, DesiredAction::Recreate);
}

#[tokio::test]
async fn test_scale_down_removes_surplus_replicas() {
    let engine = MockEngine::new();
    let three = "services:\n  web:\n    image: nginx\n    deploy:\n      replicas: 3\n";
    Orchestrator::new(load(three), engine.clone()).up(UpOptions::default()).await.unwrap();
    assert_eq!(engine.list_containers("pk").await.unwrap().len(), 3);

    let one = three.replace("replicas: 3", "replicas: 1");
    Orchestrator::new(load(&one), engine.clone()).up(UpOptions::default()).await.unwrap();
    let names: Vec<String> = engine.list_containers("pk").await.unwrap().into_iter().map(|c| c.name).collect();
    assert_eq!(names, vec!["pk-web-1"]);
}
