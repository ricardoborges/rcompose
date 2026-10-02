use rcompose_engine::engine::CreateOptions;
use rcompose_engine::wslc::command::build_create_args;
use rcompose_spec::model::{Healthcheck, PortMapping, VolumeMount};

fn pos(args: &[String], value: &str) -> usize {
    args.iter().position(|a| a == value).unwrap_or_else(|| panic!("{} not in {:?}", value, args))
}

#[test]
fn test_build_create_args() {
    let mut opts = CreateOptions {
        name: "test-app-1".to_string(),
        image: "nginx:alpine".to_string(),
        network: Some("test_default".to_string()),
        network_aliases: vec!["app".to_string()],
        stop_timeout: Some(5),
        ..Default::default()
    };
    opts.labels.insert("com.docker.compose.project".to_string(), "myproj".to_string());
    opts.ports.extend(PortMapping::parse("8080:80").unwrap());
    opts.volumes.push(VolumeMount::parse(r"C:\site:/usr/share/nginx/html:ro").unwrap());
    opts.command = Some(vec!["nginx".to_string(), "-g".to_string(), "daemon off;".to_string()]);

    let args = build_create_args(&opts);
    assert_eq!(args[0], "create");
    assert_eq!(args[pos(&args, "--name") + 1], "test-app-1");
    assert_eq!(args[pos(&args, "-p") + 1], "8080:80");
    assert_eq!(args[pos(&args, "-v") + 1], r"C:\site:/usr/share/nginx/html:ro");
    assert_eq!(args[pos(&args, "-l") + 1], "com.docker.compose.project=myproj");
    assert_eq!(args[pos(&args, "--network") + 1], "test_default");
    assert_eq!(args[pos(&args, "--network-alias") + 1], "app");
    assert_eq!(args[pos(&args, "--stop-timeout") + 1], "5");

    // Command follows the image
    let img_pos = pos(&args, "nginx:alpine");
    assert_eq!(&args[img_pos + 1..], &["nginx", "-g", "daemon off;"]);
}

#[test]
fn test_healthcheck_args() {
    let opts = CreateOptions {
        name: "db-1".to_string(),
        image: "postgres".to_string(),
        healthcheck: Some(Healthcheck {
            test: vec!["CMD".into(), "curl".into(), "-f".into(), "http://localhost:3000/api v1".into()],
            interval: Some("10s".into()),
            retries: Some(10),
            ..Default::default()
        }),
        ..Default::default()
    };
    let args = build_create_args(&opts);
    assert_eq!(args[pos(&args, "--health-cmd") + 1], "curl -f 'http://localhost:3000/api v1'");
    assert_eq!(args[pos(&args, "--health-interval") + 1], "10s");
    assert_eq!(args[pos(&args, "--health-retries") + 1], "10");

    let disabled = CreateOptions {
        healthcheck: Some(Healthcheck { disable: true, ..Default::default() }),
        ..opts
    };
    assert!(build_create_args(&disabled).contains(&"--no-healthcheck".to_string()));
}
