use rcompose_core::dag::*;
use rcompose_spec::model::{Project, Service};

#[test]
fn test_dag_linear_dependencies() {
    let mut project = Project::new("test");

    let mut db = Service::default();
    db.name = "db".to_string();

    let mut api = Service::default();
    api.name = "api".to_string();
    api.depends_on = vec!["db".to_string()];

    let mut web = Service::default();
    web.name = "web".to_string();
    web.depends_on = vec!["api".to_string()];

    project.services.insert("db".to_string(), db);
    project.services.insert("api".to_string(), api);
    project.services.insert("web".to_string(), web);

    let dag = DependencyGraph::from_project(&project).unwrap();
    let batches = dag.execution_batches();

    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0], vec!["db"]);
    assert_eq!(batches[1], vec!["api"]);
    assert_eq!(batches[2], vec!["web"]);

    let down_batches = dag.shutdown_batches();
    assert_eq!(down_batches.len(), 3);
    assert_eq!(down_batches[0], vec!["web"]);
    assert_eq!(down_batches[1], vec!["api"]);
    assert_eq!(down_batches[2], vec!["db"]);
}

#[test]
fn test_dag_concurrent_batches() {
    let mut project = Project::new("test");

    let mut db = Service::default();
    db.name = "db".to_string();

    let mut redis = Service::default();
    redis.name = "redis".to_string();

    let mut api = Service::default();
    api.name = "api".to_string();
    api.depends_on = vec!["db".to_string(), "redis".to_string()];

    project.services.insert("db".to_string(), db);
    project.services.insert("redis".to_string(), redis);
    project.services.insert("api".to_string(), api);

    let dag = DependencyGraph::from_project(&project).unwrap();
    let batches = dag.execution_batches();

    assert_eq!(batches.len(), 2);
    // db and redis can run in parallel
    let mut layer0 = batches[0].clone();
    layer0.sort();
    assert_eq!(layer0, vec!["db", "redis"]);
    assert_eq!(batches[1], vec!["api"]);
}

#[test]
fn test_dag_circular_dependency() {
    let mut project = Project::new("test");

    let mut a = Service::default();
    a.name = "a".to_string();
    a.depends_on = vec!["b".to_string()];

    let mut b = Service::default();
    b.name = "b".to_string();
    b.depends_on = vec!["a".to_string()];

    project.services.insert("a".to_string(), a);
    project.services.insert("b".to_string(), b);

    let res = DependencyGraph::from_project(&project);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(matches!(err, DagError::CircularDependency(_)));
}
