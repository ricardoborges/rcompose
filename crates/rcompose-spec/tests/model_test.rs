use rcompose_spec::model::*;

#[test]
fn test_port_mapping_parsing() {
    let p1 = PortMapping::parse("8080:80").unwrap();
    assert_eq!(p1.target, 80);
    assert_eq!(p1.published.as_deref(), Some("8080"));
    assert_eq!(p1.protocol, "tcp");
    assert_eq!(p1.to_flag(), "8080:80");

    let p2 = PortMapping::parse("127.0.0.1:53:53/udp").unwrap();
    assert_eq!(p2.target, 53);
    assert_eq!(p2.published.as_deref(), Some("127.0.0.1:53"));
    assert_eq!(p2.protocol, "udp");
    assert_eq!(p2.to_flag(), "127.0.0.1:53:53/udp");

    let p3 = PortMapping::parse("3000").unwrap();
    assert_eq!(p3.target, 3000);
    assert_eq!(p3.published, None);
    assert_eq!(p3.to_flag(), "3000");
}

#[test]
fn test_volume_mount_parsing() {
    let v1 = VolumeMount::parse("./app:/app:ro").unwrap();
    assert_eq!(v1.mount_type, VolumeType::Bind);
    assert_eq!(v1.source.as_deref(), Some("./app"));
    assert_eq!(v1.target, "/app");
    assert!(v1.read_only);

    let v2 = VolumeMount::parse("db-data:/var/lib/postgresql/data").unwrap();
    assert_eq!(v2.mount_type, VolumeType::Volume);
    assert_eq!(v2.source.as_deref(), Some("db-data"));
    assert_eq!(v2.target, "/var/lib/postgresql/data");
    assert!(!v2.read_only);

    let v3 = VolumeMount::parse("/tmp/cache").unwrap();
    assert_eq!(v3.target, "/tmp/cache");
}

#[test]
fn test_service_config_hash() {
    let mut svc1 = Service::default();
    svc1.image = Some("nginx:alpine".to_string());
    svc1.ports.push(PortMapping::parse("80:80").unwrap());

    let mut svc2 = Service::default();
    svc2.image = Some("nginx:alpine".to_string());
    svc2.ports.push(PortMapping::parse("80:80").unwrap());

    assert_eq!(svc1.config_hash(), svc2.config_hash());

    svc2.environment.insert("ENV_VAR".to_string(), Some("1".to_string()));
    assert_ne!(svc1.config_hash(), svc2.config_hash());
}

#[test]
fn test_project_container_name() {
    let project = Project::new("myproject");
    let mut svc = Service::default();
    svc.name = "web".to_string();

    assert_eq!(project.container_name(&svc, 1), "myproject-web-1");

    svc.container_name = Some("custom-web".to_string());
    assert_eq!(project.container_name(&svc, 1), "custom-web");
}
