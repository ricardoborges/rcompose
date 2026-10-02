use rcompose_spec::model::*;

#[test]
fn test_port_mapping_parsing() {
    let p1 = PortMapping::parse("8080:80").unwrap().remove(0);
    assert_eq!(p1.target, 80);
    assert_eq!(p1.published.as_deref(), Some("8080"));
    assert_eq!(p1.protocol, "tcp");
    assert_eq!(p1.to_flag(), "8080:80");

    let p2 = PortMapping::parse("127.0.0.1:53:53/udp").unwrap().remove(0);
    assert_eq!(p2.target, 53);
    assert_eq!(p2.published.as_deref(), Some("127.0.0.1:53"));
    assert_eq!(p2.protocol, "udp");
    assert_eq!(p2.to_flag(), "127.0.0.1:53:53/udp");

    let p3 = PortMapping::parse("3000").unwrap().remove(0);
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
fn test_windows_volume_paths() {
    let v = VolumeMount::parse(r"C:\data:/data:ro").unwrap();
    assert_eq!(v.mount_type, VolumeType::Bind);
    assert_eq!(v.source.as_deref(), Some(r"C:\data"));
    assert_eq!(v.target, "/data");
    assert!(v.read_only);

    let v = VolumeMount::parse("D:/projects/app:/app").unwrap();
    assert_eq!(v.source.as_deref(), Some("D:/projects/app"));
    assert_eq!(v.target, "/app");
}

#[test]
fn test_port_ranges() {
    let ports = PortMapping::parse("127.0.0.1:8000-8001:9000-9001/udp").unwrap();
    let flags: Vec<String> = ports.iter().map(|p| p.to_flag()).collect();
    assert_eq!(flags, vec!["127.0.0.1:8000:9000/udp", "127.0.0.1:8001:9001/udp"]);
    assert!(PortMapping::parse("8000-8002:9000-9001").is_err());
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
