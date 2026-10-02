use rcompose_spec::loader::*;
use std::collections::HashMap;

#[test]
fn test_load_compose_yaml_string() {
    let compose_yaml = r#"
version: "3.8"
services:
  db:
    image: postgres:16
    environment:
      POSTGRES_PASSWORD: ${DB_PASS:-secret}
    volumes:
      - db_data:/var/lib/postgresql/data
  web:
    image: nginx:alpine
    ports:
      - "8080:80"
    depends_on:
      - db
volumes:
  db_data:
"#;

    let mut env = HashMap::new();
    env.insert("DB_PASS".to_string(), "supersecret".to_string());

    let opts = LoadOptions {
        project_name: Some("testproj".to_string()),
        env: Some(env),
        ..Default::default()
    };

    let project = load_project_from_str(compose_yaml, None, opts).unwrap();
    assert_eq!(project.name, "testproj");
    assert_eq!(project.services.len(), 2);

    let db = project.services.get("db").unwrap();
    assert_eq!(db.image.as_deref(), Some("postgres:16"));
    assert_eq!(
        db.environment.get("POSTGRES_PASSWORD").and_then(|v| v.as_deref()),
        Some("supersecret")
    );

    let web = project.services.get("web").unwrap();
    assert_eq!(web.depends_on, vec!["db"]);
    assert_eq!(web.ports.len(), 1);
    assert_eq!(web.ports[0].target, 80);
    assert_eq!(web.ports[0].published.as_deref(), Some("8080"));

    // Sorted services should have db before web
    let sorted = project.sorted_services(None).unwrap();
    let names: Vec<&str> = sorted.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["db", "web"]);
}

#[test]
fn test_load_with_rcompose_extension() {
    let compose_yaml = r#"
services:
  worker:
    image: my-ai-worker:latest
"#;

    let rcompose_yaml = r#"
services:
  worker:
    wsl:
      gpus: all
      session: ai-session
      memory_mb: 8192
"#;

    let opts = LoadOptions::default();
    let project = load_project_from_str(compose_yaml, Some(rcompose_yaml), opts).unwrap();
    let worker = project.services.get("worker").unwrap();

    assert_eq!(worker.gpus.as_deref(), Some("all"));
    assert_eq!(worker.wsl_session.as_deref(), Some("ai-session"));
    assert_eq!(worker.mem_limit.as_deref(), Some("8192m"));
}
