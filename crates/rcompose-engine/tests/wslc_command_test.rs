use rcompose_engine::engine::RunOptions;
use rcompose_engine::wslc::command::build_run_args;
use rcompose_spec::model::PortMapping;

#[test]
fn test_build_run_args() {
    let mut opts = RunOptions {
        name: "test-app-1".to_string(),
        image: "nginx:alpine".to_string(),
        detach: true,
        ..Default::default()
    };
    opts.labels.insert("com.docker.compose.project".to_string(), "myproj".to_string());
    opts.ports.push(PortMapping::parse("8080:80").unwrap());
    opts.command = Some(vec!["nginx".to_string(), "-g".to_string(), "daemon off;".to_string()]);

    let args = build_run_args(&opts);
    assert_eq!(args[0], "run");
    assert!(args.contains(&"-d".to_string()));
    assert!(args.contains(&"--name".to_string()));
    assert!(args.contains(&"test-app-1".to_string()));
    assert!(args.contains(&"-p".to_string()));
    assert!(args.contains(&"8080:80".to_string()));
    assert!(args.contains(&"nginx:alpine".to_string()));
    assert!(args.contains(&"-l".to_string()));
    assert!(args.contains(&"com.docker.compose.project=myproj".to_string()));

    // Command should be at the end
    let img_pos = args.iter().position(|r| r == "nginx:alpine").unwrap();
    assert_eq!(&args[img_pos + 1..], &["nginx", "-g", "daemon off;"]);
}
