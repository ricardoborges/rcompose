use rcompose_spec::loader::*;
use rcompose_spec::model::*;
use std::collections::HashMap;

fn opts_with_env(vars: &[(&str, &str)]) -> LoadOptions {
    LoadOptions {
        project_name: Some("proj".to_string()),
        env: Some(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()),
        ..Default::default()
    }
}

fn load(yaml: &str) -> Project {
    load_project_from_str(yaml, opts_with_env(&[])).unwrap()
}

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

    let project = load_project_from_str(compose_yaml, opts).unwrap();
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

    // Named volumes are prefixed with the project name
    assert_eq!(db.volumes[0].source.as_deref(), Some("testproj_db_data"));
}

#[test]
fn test_load_resource_fields() {
    let compose_yaml = r#"
services:
  worker:
    image: my-ai-worker:latest
    gpus: all
    mem_limit: 8192m
    cpus: 2
"#;

    let project = load_project_from_str(compose_yaml, opts_with_env(&[])).unwrap();
    let worker = project.services.get("worker").unwrap();

    assert_eq!(worker.gpus.as_deref(), Some("all"));
    assert_eq!(worker.mem_limit.as_deref(), Some("8192m"));
    assert_eq!(worker.cpus.as_deref(), Some("2"));
}

#[test]
fn test_yaml_merge_keys() {
    let project = load(
        r#"
x-base: &base
  image: nginx
  environment:
    A: "1"
services:
  web:
    <<: *base
    ports: ["80"]
"#,
    );
    let web = &project.services["web"];
    assert_eq!(web.image.as_deref(), Some("nginx"));
    assert_eq!(web.environment["A"].as_deref(), Some("1"));
}

#[test]
fn test_interpolation_skips_comments() {
    let project = load("# ${SECRET:?must be set}\nservices:\n  web:\n    image: nginx\n");
    assert!(project.services.contains_key("web"));
}

#[test]
fn test_long_syntax_ports_and_volumes() {
    let project = load(
        r#"
services:
  web:
    image: nginx
    ports:
      - target: 80
        published: 8080
      - "9000-9001:9000-9001"
    volumes:
      - type: volume
        source: data
        target: /data
      - type: tmpfs
        target: /cache
volumes:
  data:
"#,
    );
    let web = &project.services["web"];
    let flags: Vec<String> = web.ports.iter().map(|p| p.to_flag()).collect();
    assert_eq!(flags, vec!["8080:80", "9000:9000", "9001:9001"]);
    assert_eq!(web.volumes[0].source.as_deref(), Some("proj_data"));
    assert_eq!(web.volumes[1].mount_type, VolumeType::Tmpfs);
}

#[test]
fn test_undefined_volume_and_network_are_errors() {
    let vol = load_project_from_str(
        "services:\n  web:\n    image: nginx\n    volumes: [\"data:/data\"]\n",
        opts_with_env(&[]),
    );
    assert!(vol.unwrap_err().to_string().contains("undefined volume 'data'"));

    let net = load_project_from_str(
        "services:\n  web:\n    image: nginx\n    networks: [back]\n",
        opts_with_env(&[]),
    );
    assert!(net.unwrap_err().to_string().contains("undefined network 'back'"));
}

#[test]
fn test_bind_paths_resolve_against_project_dir() {
    let dir = std::env::temp_dir().join("rcompose-bind-test");
    let opts = LoadOptions { working_dir: Some(dir.clone()), ..opts_with_env(&[]) };
    let project = load_project_from_str(
        r#"
services:
  web:
    image: nginx
    volumes:
      - ./site:/usr/share/nginx/html:ro
      - C:\data:/data
"#,
        opts,
    )
    .unwrap();
    let web = &project.services["web"];
    assert_eq!(web.volumes[0].source.as_deref(), Some(dir.join("site").to_str().unwrap()));
    assert!(web.volumes[0].read_only);
    assert_eq!(web.volumes[1].source.as_deref(), Some(r"C:\data"));
    assert_eq!(web.volumes[1].target, "/data");
}

#[cfg(windows)]
#[test]
fn test_linux_only_bind_is_skipped_with_warning() {
    let project = load(
        "services:\n  web:\n    image: nginx\n    volumes: [\"/var/run/docker.sock:/var/run/docker.sock\"]\n",
    );
    assert!(project.services["web"].volumes.is_empty());
    assert!(project.warnings.iter().any(|w| w.contains("/var/run/docker.sock")));
}

#[test]
fn test_project_name_precedence() {
    let yaml = "name: From-File\nservices:\n  web:\n    image: nginx\n";
    let from_file =
        load_project_from_str(yaml, LoadOptions { project_name: None, ..opts_with_env(&[]) }).unwrap();
    assert_eq!(from_file.name, "from-file");

    let from_env = load_project_from_str(
        yaml,
        LoadOptions { project_name: None, ..opts_with_env(&[("COMPOSE_PROJECT_NAME", "envname")]) },
    )
    .unwrap();
    assert_eq!(from_env.name, "envname");

    let from_flag =
        load_project_from_str(yaml, opts_with_env(&[("COMPOSE_PROJECT_NAME", "envname")])).unwrap();
    assert_eq!(from_flag.name, "proj");
}

#[test]
fn test_profiles_filter_services() {
    let yaml = r#"
services:
  api:
    image: api
  builder:
    image: builder
    profiles: ["build"]
"#;
    let default = load(yaml);
    assert!(default.services.contains_key("api"));
    assert!(!default.services.contains_key("builder"));
    assert!(default.disabled_services.contains("builder"));

    let with_profile =
        load_project_from_str(yaml, LoadOptions { profiles: vec!["build".into()], ..opts_with_env(&[]) })
            .unwrap();
    assert!(with_profile.services.contains_key("builder"));
}

#[test]
fn test_depends_on_conditions_and_validation() {
    let project = load(
        r#"
services:
  db:
    image: postgres
    healthcheck:
      test: ["CMD-SHELL", "pg_isready"]
      interval: 3s
      retries: 20
  init:
    image: alpine
  app:
    image: app
    depends_on:
      db:
        condition: service_healthy
      init:
        condition: service_completed_successfully
      cache:
        condition: service_started
        required: false
"#,
    );
    let app = &project.services["app"];
    assert_eq!(app.depends_on, vec!["db", "init"]);
    assert_eq!(app.dependency_condition("db"), DependencyCondition::ServiceHealthy);
    assert_eq!(app.dependency_condition("init"), DependencyCondition::ServiceCompletedSuccessfully);
    let hc = project.services["db"].healthcheck.as_ref().unwrap();
    assert_eq!(hc.test, vec!["CMD-SHELL", "pg_isready"]);
    assert_eq!(hc.retries, Some(20));

    let missing = load_project_from_str(
        "services:\n  app:\n    image: app\n    depends_on: [db]\n",
        opts_with_env(&[]),
    );
    assert!(missing.unwrap_err().to_string().contains("'db', which is not defined"));
}

#[test]
fn test_networks_with_aliases_and_external() {
    let project = load(
        r#"
services:
  gitea:
    image: gitea
    networks:
      default:
      coolify:
        aliases: [git]
networks:
  coolify:
    name: coolify
    external: true
"#,
    );
    let gitea = &project.services["gitea"];
    assert_eq!(gitea.networks, vec!["proj_default", "coolify"]);
    assert_eq!(gitea.network_aliases["coolify"], vec!["git"]);
    assert!(project.networks["coolify"].external);
    assert!(!project.networks["default"].external);
}

#[test]
fn test_env_file_long_syntax_and_command_string() {
    let dir = std::env::temp_dir().join("rcompose-envfile-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("app.env"), "FROM_FILE=1\nOVERRIDE=file\n").unwrap();
    let project = load_project_from_str(
        r#"
services:
  app:
    image: app
    command: sh -c "echo 'hi there'"
    env_file:
      - app.env
      - path: missing.env
        required: false
    environment:
      OVERRIDE: env
"#,
        LoadOptions { working_dir: Some(dir), ..opts_with_env(&[]) },
    )
    .unwrap();
    let app = &project.services["app"];
    assert_eq!(app.environment["FROM_FILE"].as_deref(), Some("1"));
    assert_eq!(app.environment["OVERRIDE"].as_deref(), Some("env"));
    assert_eq!(app.command.as_deref().unwrap(), ["sh", "-c", "echo 'hi there'"]);
}

#[test]
fn test_unsupported_keys_warn() {
    let project = load("services:\n  app:\n    image: app\n    privileged: true\n    restart: always\n");
    assert!(project.warnings.iter().any(|w| w.contains("'privileged'")));
    assert!(project.warnings.iter().any(|w| w.contains("restart policy 'always'")));
}
